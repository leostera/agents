use anyhow::Result;
use borg_db::BorgDb;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info};

use crate::store::TaskGraphStore;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct TaskGraphSupervisor {
    store: TaskGraphStore,
    known_statuses: Arc<RwLock<HashMap<String, String>>>,
    poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct TaskNotification {
    pub event_type: String,
    pub task_uri: String,
    pub parent_uri: Option<String>,
    pub old_status: Option<String>,
    pub new_status: String,
    pub data: serde_json::Value,
}

impl TaskGraphSupervisor {
    pub fn new(db: BorgDb) -> Self {
        Self {
            store: TaskGraphStore::new(db),
            known_statuses: Arc::new(RwLock::new(HashMap::new())),
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub async fn start(&self) {
        info!("TaskGraphSupervisor starting");

        let store = self.store.clone();
        let known = self.known_statuses.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            if let Err(err) = Self::initialize_statuses(&store, &known).await {
                error!(target: "borg_taskgraph", error = %err, "failed to initialize task statuses");
            }

            let mut ticker = time::interval(poll_interval);
            loop {
                ticker.tick().await;
                if let Err(err) = Self::check_task_status_changes(&store, &known).await {
                    error!(target: "borg_taskgraph", error = %err, "error checking pending tasks");
                }
            }
        });
    }

    async fn initialize_statuses(
        store: &TaskGraphStore,
        known: &Arc<RwLock<HashMap<String, String>>>,
    ) -> Result<()> {
        let tasks = store.list_all_task_uris().await?;
        let mut statuses = known.write().await;
        for task in tasks {
            statuses.insert(task.0, task.1);
        }
        info!(target: "borg_taskgraph", task_count = statuses.len(), "initialized task statuses");
        Ok(())
    }

    async fn check_task_status_changes(
        store: &TaskGraphStore,
        known: &Arc<RwLock<HashMap<String, String>>>,
    ) -> Result<()> {
        let current_tasks = store.list_all_task_uris().await?;
        let mut statuses = known.write().await;

        for (uri, new_status) in current_tasks {
            let old_status = statuses.get(&uri).cloned();

            if old_status.as_ref() != Some(&new_status) {
                let notification = TaskNotification {
                    event_type: "child_status_changed".to_string(),
                    task_uri: uri.clone(),
                    parent_uri: store.get_task_parent(&uri).await?,
                    old_status: old_status.clone(),
                    new_status: new_status.clone(),
                    data: json!({}),
                };

                if let Some(ref parent_uri) = notification.parent_uri {
                    Self::notify_parent(&notification, parent_uri).await?;
                }

                statuses.insert(uri.clone(), new_status);
                debug!(target: "borg_taskgraph", task_uri = %notification.task_uri, old_status = ?old_status, new_status = %notification.new_status, "task status changed");
            }
        }

        Ok(())
    }

    async fn notify_parent(notification: &TaskNotification, parent_uri: &str) -> Result<()> {
        let event_json = json!({
            "event_type": notification.event_type,
            "child_uri": notification.task_uri,
            "old_status": notification.old_status,
            "new_status": notification.new_status,
            "data": notification.data,
        });

        info!(target: "borg_taskgraph", parent_uri = %parent_uri, child_uri = %notification.task_uri, "notifying parent agent about child status change: {}", event_json);

        Ok(())
    }

    pub async fn shutdown(&self) {
        info!("TaskGraphSupervisor shutting down");
    }
}

impl TaskGraphStore {
    pub async fn list_all_task_uris(&self) -> Result<Vec<(String, String)>> {
        let mut tx = self.db().pool().begin().await?;
        let rows: Vec<(String, String)> =
            sqlx::query_as::<_, (String, String)>("SELECT uri, status FROM taskgraph_tasks")
                .fetch_all(tx.as_mut())
                .await?;
        tx.commit().await?;

        Ok(rows)
    }

    pub async fn get_task_parent(&self, uri: &str) -> Result<Option<String>> {
        let mut tx = self.db().pool().begin().await?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT parent_uri FROM taskgraph_tasks WHERE uri = ?1")
                .bind(uri)
                .fetch_optional(tx.as_mut())
                .await?;
        tx.commit().await?;

        Ok(row.and_then(|(s,)| if s.is_empty() { None } else { Some(s) }))
    }
}
