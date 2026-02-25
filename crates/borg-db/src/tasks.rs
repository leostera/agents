use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use tracing::{debug, info};
use uuid::Uuid;

use borg_core::{Task, TaskEvent};

use crate::utils::{parse_ts, row_to_task};
use crate::{BorgDb, NewTask};

impl BorgDb {
    pub async fn enqueue_task(&self, task: NewTask) -> Result<String> {
        let task_id = Uuid::new_v4().to_string();
        let payload_json = task.payload.to_string();
        let parent_task_id = task.parent_task_id;
        let kind = task.kind.as_str().to_string();
        let now = Utc::now().to_rfc3339();
        info!(target: "borg_db", task_id, kind, "enqueueing task");

        self.conn
            .execute(
                r#"
                INSERT INTO tasks(task_id, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error)
                VALUES(?1, ?2, 'queued', ?3, ?4, ?5, ?6, NULL, 0, NULL)
                "#,
                (
                    task_id.clone(),
                    parent_task_id,
                    kind.clone(),
                    payload_json,
                    now.clone(),
                    now,
                ),
            )
            .await
            .context("failed to enqueue task")?;

        self.log_event(
            &task_id,
            "task_created",
            serde_json::json!({ "kind": kind }),
        )
        .await?;

        Ok(task_id)
    }

    pub async fn list_tasks(&self, status: Option<String>, limit: usize) -> Result<Vec<Task>> {
        let limit = i64::try_from(limit).unwrap_or(100);

        let mut rows = if let Some(status) = status {
            self.conn
                .query(
                    "SELECT task_id, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error FROM tasks WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2",
                    (status, limit),
                )
                .await
                .context("failed to query task list by status")?
        } else {
            self.conn
                .query(
                    "SELECT task_id, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error FROM tasks ORDER BY created_at DESC LIMIT ?1",
                    (limit,),
                )
                .await
                .context("failed to query task list")?
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading task row")? {
            out.push(row_to_task(&row)?);
        }
        Ok(out)
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error FROM tasks WHERE task_id = ?1",
                (task_id.to_string(),),
            )
            .await
            .context("failed to query task")?;

        let maybe_row = rows.next().await.context("failed reading task row")?;
        maybe_row.map(|row| row_to_task(&row)).transpose()
    }

    pub async fn get_task_events(&self, task_id: &str) -> Result<Vec<TaskEvent>> {
        let mut rows = self
            .conn
            .query(
                "SELECT event_id, task_id, ts, type, payload_json FROM task_events WHERE task_id = ?1 ORDER BY ts ASC",
                (task_id.to_string(),),
            )
            .await
            .context("failed to query task events")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading task event row")? {
            let ts: String = row.get(2)?;
            out.push(TaskEvent {
                event_id: row.get(0)?,
                task_id: row.get(1)?,
                ts: parse_ts(&ts)?,
                event_type: row.get(3)?,
                payload: serde_json::from_str(&row.get::<String>(4)?).unwrap_or(Value::Null),
            });
        }
        Ok(out)
    }

    pub async fn claim_next_runnable_task(&self, worker_id: &str) -> Result<Option<Task>> {
        let worker_id = worker_id.to_owned();
        let now = Utc::now().to_rfc3339();
        debug!(target: "borg_db", worker_id, "claiming next runnable task");

        let mut rows = self
            .conn
            .query(
                r#"
                SELECT t.task_id
                FROM tasks t
                WHERE t.status = 'queued'
                  AND NOT EXISTS (
                    SELECT 1
                    FROM deps d
                    JOIN tasks dep ON dep.task_id = d.depends_on_task_id
                    WHERE d.task_id = t.task_id
                      AND dep.status != 'succeeded'
                  )
                ORDER BY t.created_at ASC
                LIMIT 1
                "#,
                (),
            )
            .await
            .context("failed to query runnable task")?;

        let Some(row) = rows
            .next()
            .await
            .context("failed reading runnable task row")?
        else {
            return Ok(None);
        };

        let task_id: String = row.get(0)?;
        let updated = self
            .conn
            .execute(
                "UPDATE tasks SET status = 'running', claimed_by = ?1, updated_at = ?2, attempts = attempts + 1 WHERE task_id = ?3 AND status = 'queued'",
                (worker_id, now, task_id.clone()),
            )
            .await
            .context("failed to claim task")?;

        if updated == 0 {
            return Ok(None);
        }

        self.get_task(&task_id).await
    }

    pub async fn requeue_running_tasks(&self) -> Result<u64> {
        info!(target: "borg_db", "requeueing running tasks");
        let now = Utc::now().to_rfc3339();
        let updated = self
            .conn
            .execute(
                "UPDATE tasks SET status = 'queued', claimed_by = NULL, updated_at = ?1 WHERE status = 'running'",
                (now,),
            )
            .await
            .context("failed to requeue running tasks")?;
        Ok(updated)
    }

    pub async fn list_recoverable_task_ids(&self) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id FROM tasks WHERE status = 'queued' ORDER BY created_at ASC",
                (),
            )
            .await
            .context("failed to list recoverable task ids")?;

        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .context("failed reading recoverable task row")?
        {
            out.push(row.get(0)?);
        }
        Ok(out)
    }

    pub async fn claim_task_by_id(&self, worker_id: &str, task_id: &str) -> Result<Option<Task>> {
        let worker_id = worker_id.to_owned();
        let task_id = task_id.to_owned();
        let now = Utc::now().to_rfc3339();
        debug!(target: "borg_db", worker_id, task_id, "claiming task by id");

        let mut rows = self
            .conn
            .query(
                r#"
                SELECT t.task_id
                FROM tasks t
                WHERE t.task_id = ?1
                  AND t.status = 'queued'
                  AND NOT EXISTS (
                    SELECT 1
                    FROM deps d
                    JOIN tasks dep ON dep.task_id = d.depends_on_task_id
                    WHERE d.task_id = ?1
                      AND dep.status != 'succeeded'
                  )
                "#,
                (task_id.clone(),),
            )
            .await
            .context("failed to query claimable task by id")?;

        let Some(_row) = rows
            .next()
            .await
            .context("failed reading claimable task row")?
        else {
            return Ok(None);
        };

        let updated = self
            .conn
            .execute(
                "UPDATE tasks SET status = 'running', claimed_by = ?1, updated_at = ?2, attempts = attempts + 1 WHERE task_id = ?3 AND status = 'queued'",
                (worker_id, now, task_id.clone()),
            )
            .await
            .context("failed to claim task by id")?;

        if updated == 0 {
            return Ok(None);
        }

        self.get_task(&task_id).await
    }

    pub async fn complete_task(&self, task_id: &str, result: Value) -> Result<()> {
        let payload_json = result.to_string();
        let now = Utc::now().to_rfc3339();
        info!(target: "borg_db", task_id, "marking task succeeded");

        self.conn
            .execute(
                "UPDATE tasks SET status = 'succeeded', updated_at = ?1, last_error = NULL WHERE task_id = ?2",
                (now.clone(), task_id.to_string()),
            )
            .await
            .context("failed updating task to succeeded")?;

        self.conn
            .execute(
                "INSERT INTO task_events(event_id, task_id, ts, type, payload_json) VALUES(?1, ?2, ?3, 'task_succeeded', ?4)",
                (Uuid::now_v7().to_string(), task_id.to_string(), now, payload_json),
            )
            .await
            .context("failed inserting success task event")?;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error_message: String) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let err_json = serde_json::json!({ "error": error_message.clone() }).to_string();
        info!(target: "borg_db", task_id, error = error_message, "marking task failed");

        self.conn
            .execute(
                "UPDATE tasks SET status = 'failed', updated_at = ?1, last_error = ?2 WHERE task_id = ?3",
                (now.clone(), error_message, task_id.to_string()),
            )
            .await
            .context("failed updating task to failed")?;

        self.conn
            .execute(
                "INSERT INTO task_events(event_id, task_id, ts, type, payload_json) VALUES(?1, ?2, ?3, 'task_failed', ?4)",
                (Uuid::now_v7().to_string(), task_id.to_string(), now, err_json),
            )
            .await
            .context("failed inserting failed task event")?;

        Ok(())
    }

    pub async fn log_event(&self, task_id: &str, event_type: &str, payload: Value) -> Result<()> {
        let event_id = Uuid::now_v7().to_string();
        let payload = payload.to_string();
        let now = Utc::now().to_rfc3339();

        debug!(target: "borg_db", task_id, event_type, "writing task event");
        self.conn
            .execute(
                "INSERT INTO task_events(event_id, task_id, ts, type, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    event_id,
                    task_id.to_string(),
                    now,
                    event_type.to_string(),
                    payload,
                ),
            )
            .await
            .context("failed to write task event")?;
        Ok(())
    }

    pub async fn add_dependency(&self, task_id: &str, depends_on_task_id: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO deps(task_id, depends_on_task_id) VALUES(?1, ?2)",
                (task_id.to_string(), depends_on_task_id.to_string()),
            )
            .await
            .context("failed to insert task dependency")?;
        Ok(())
    }
}
