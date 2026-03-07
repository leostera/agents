use anyhow::Result;
use borg_core::Uri;
use borg_db::BorgDb;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::model::{TaskEventData, TaskStatus};
use crate::store::TaskGraphStore;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct TaskGraphSupervisor {
    store: TaskGraphStore,
    known_statuses: Arc<RwLock<HashMap<String, String>>>,
    dispatched_tasks: Arc<RwLock<HashSet<String>>>,
    dispatch_tx: Option<mpsc::Sender<TaskDispatch>>,
    poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct TaskNotification {
    pub event_type: String,
    pub task_uri: String,
    pub parent_uri: Option<String>,
    pub old_status: Option<String>,
    pub new_status: String,
    pub data: TaskEventData,
}

#[derive(Debug, Clone)]
pub struct TaskDispatch {
    pub task_uri: Uri,
    pub title: String,
    pub description: String,
    pub definition_of_done: String,
    pub assignee_actor_id: Uri,
}

impl TryFrom<crate::model::TaskRecord> for TaskDispatch {
    type Error = anyhow::Error;

    fn try_from(value: crate::model::TaskRecord) -> Result<Self> {
        Ok(Self {
            task_uri: Uri::parse(&value.uri)
                .map_err(|_| anyhow::anyhow!("task.invalid_uri: {}", value.uri))?,
            title: value.title,
            description: value.description,
            definition_of_done: value.definition_of_done,
            assignee_actor_id: Uri::parse(&value.assignee_actor_id).map_err(|_| {
                anyhow::anyhow!(
                    "task.invalid_assignee_actor_id: {}",
                    value.assignee_actor_id
                )
            })?,
        })
    }
}

impl TaskGraphSupervisor {
    pub fn new(db: BorgDb) -> Self {
        Self {
            store: TaskGraphStore::new(db),
            known_statuses: Arc::new(RwLock::new(HashMap::new())),
            dispatched_tasks: Arc::new(RwLock::new(HashSet::new())),
            dispatch_tx: None,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_dispatch(mut self, dispatch_tx: mpsc::Sender<TaskDispatch>) -> Self {
        self.dispatch_tx = Some(dispatch_tx);
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub async fn start(self) -> tokio::task::JoinHandle<Result<()>> {
        info!("TaskGraphSupervisor starting");

        let store = self.store.clone();
        let known = self.known_statuses.clone();
        let dispatched = self.dispatched_tasks.clone();
        let dispatch_tx = self.dispatch_tx.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            if let Err(err) = Self::initialize_statuses(&store, &known).await {
                error!(target: "borg_taskgraph", error = %err, "failed to initialize task statuses");
            }

            let mut ticker = time::interval(poll_interval);
            loop {
                ticker.tick().await;
                if let Err(err) = Self::check_task_status_changes(&store, &known, &dispatched).await
                {
                    error!(target: "borg_taskgraph", error = %err, "error checking pending tasks");
                }
                if let Err(err) =
                    Self::dispatch_ready_tasks(&store, &dispatched, dispatch_tx.as_ref()).await
                {
                    error!(target: "borg_taskgraph", error = %err, "error dispatching tasks");
                }
            }
        })
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
        dispatched: &Arc<RwLock<HashSet<String>>>,
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
                    data: TaskEventData::Empty {},
                };

                if let Some(ref parent_uri) = notification.parent_uri {
                    Self::notify_parent(&notification, parent_uri).await?;
                }

                statuses.insert(uri.clone(), new_status.clone());
                debug!(target: "borg_taskgraph", task_uri = %notification.task_uri, old_status = ?old_status, new_status = %notification.new_status, "task status changed");
            }

            if matches!(
                TaskStatus::parse(&new_status),
                Some(TaskStatus::Done | TaskStatus::Discarded)
            ) {
                let mut guard = dispatched.write().await;
                guard.remove(&uri);
            }
        }

        Ok(())
    }

    async fn dispatch_ready_tasks(
        store: &TaskGraphStore,
        dispatched: &Arc<RwLock<HashSet<String>>>,
        dispatch_tx: Option<&mpsc::Sender<TaskDispatch>>,
    ) -> Result<()> {
        let Some(dispatch_tx) = dispatch_tx else {
            return Ok(());
        };

        let assignee_actor_ids = store.list_assignee_actor_ids().await?;
        for assignee_actor_id in assignee_actor_ids {
            let assignee_actor_uri = match Uri::parse(&assignee_actor_id) {
                Ok(uri) => uri,
                Err(_) => {
                    warn!(
                        target: "borg_taskgraph",
                        assignee_actor_id = %assignee_actor_id,
                        "skipping task dispatch for invalid assignee actor uri"
                    );
                    continue;
                }
            };
            let assignee_exists = match store.db().get_actor(&assignee_actor_uri).await {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(err) => {
                    warn!(
                        target: "borg_taskgraph",
                        assignee_actor_id = %assignee_actor_id,
                        error = %err,
                        "failed to check assignee actor before dispatch"
                    );
                    continue;
                }
            };
            if !assignee_exists {
                debug!(
                    target: "borg_taskgraph",
                    assignee_actor_id = %assignee_actor_id,
                    "skipping task dispatch because assignee actor does not exist"
                );
                continue;
            }

            let tasks = store.next_task(&assignee_actor_id, 10).await?;
            for task in tasks {
                {
                    let mut guard = dispatched.write().await;
                    if guard.contains(&task.uri) {
                        continue;
                    }
                    guard.insert(task.uri.clone());
                }

                if task.status == TaskStatus::Pending.as_str()
                    && let Err(err) = store
                        .set_task_status(&task.assignee_actor_id, &task.uri, TaskStatus::Doing)
                        .await
                {
                    error!(
                        target: "borg_taskgraph",
                        task_uri = %task.uri,
                        error = %err,
                        "failed to mark task as doing before dispatch"
                    );
                    let mut guard = dispatched.write().await;
                    guard.remove(&task.uri);
                    continue;
                }

                let dispatch = match TaskDispatch::try_from(task.clone()) {
                    Ok(dispatch) => dispatch,
                    Err(err) => {
                        error!(
                            target: "borg_taskgraph",
                            task_uri = %task.uri,
                            assignee_actor_id = %task.assignee_actor_id,
                            error = %err,
                            "failed to encode task dispatch"
                        );
                        let mut guard = dispatched.write().await;
                        guard.remove(&task.uri);
                        continue;
                    }
                };
                if dispatch_tx.send(dispatch.clone()).await.is_err() {
                    return Ok(());
                }
                info!(
                    target: "borg_taskgraph",
                    task_uri = %dispatch.task_uri,
                    assignee_actor_id = %dispatch.assignee_actor_id,
                    "dispatched task to runtime"
                );
            }
        }

        Ok(())
    }

    async fn notify_parent(notification: &TaskNotification, parent_uri: &str) -> Result<()> {
        #[derive(serde::Serialize)]
        struct ParentNotification<'a> {
            event_type: &'a str,
            child_uri: &'a str,
            old_status: Option<&'a str>,
            new_status: &'a str,
            data: &'a TaskEventData,
        }
        let payload = ParentNotification {
            event_type: &notification.event_type,
            child_uri: &notification.task_uri,
            old_status: notification.old_status.as_deref(),
            new_status: &notification.new_status,
            data: &notification.data,
        };
        let event_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
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

    pub async fn list_assignee_actor_ids(&self) -> Result<Vec<String>> {
        let mut tx = self.db().pool().begin().await?;
        let rows: Vec<(String,)> = sqlx::query_as::<_, (String,)>(
            r#"SELECT DISTINCT assignee_actor_id
               FROM taskgraph_tasks
               WHERE status IN ('pending', 'doing')
               ORDER BY assignee_actor_id ASC"#,
        )
        .fetch_all(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(|(value,)| value).collect())
    }
}
