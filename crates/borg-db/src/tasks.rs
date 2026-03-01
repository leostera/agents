use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use serde_json::Value;
use tracing::{debug, info};

use borg_core::{Event, Task, TaskEvent, Uri, uri};

use crate::utils::parse_ts;
use crate::{BorgDb, NewTask};

fn parse_task_status(raw: &str) -> Result<borg_core::TaskStatus> {
    borg_core::TaskStatus::from_str(raw).ok_or_else(|| anyhow!("invalid task status: {raw}"))
}

fn parse_task_kind(raw: &str) -> Result<borg_core::TaskKind> {
    borg_core::TaskKind::from_str(raw).ok_or_else(|| anyhow!("invalid task kind: {raw}"))
}

impl BorgDb {
    pub async fn enqueue_task(&self, task: NewTask) -> Result<Uri> {
        let task_id = uri!("borg", "task");
        let payload_json = task.payload.to_string();
        let parent_task_id = task.parent_task_id;
        let kind = task.kind.as_str().to_string();
        let now = Utc::now().to_rfc3339();
        let task_id_raw = task_id.to_string();
        let parent_task_id_raw = parent_task_id.map(|id| id.to_string());
        let created_at = now.clone();
        let updated_at = now;
        info!(target: "borg_db", task_id = %task_id, kind, "enqueueing task");

        sqlx::query!(
            r#"
            INSERT INTO tasks(task_id, parent_task_id, status, kind, payload_json, created_at, updated_at, claimed_by, attempts, last_error)
            VALUES(?1, ?2, 'queued', ?3, ?4, ?5, ?6, NULL, 0, NULL)
            "#,
            task_id_raw,
            parent_task_id_raw,
            kind,
            payload_json,
            created_at,
            updated_at,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to enqueue task")?;

        self.log_event(Event::TaskCreated {
            task_id: task_id.clone(),
            kind: task.kind,
        })
        .await?;

        Ok(task_id)
    }

    pub async fn list_tasks(&self, status: Option<String>, limit: usize) -> Result<Vec<Task>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"SELECT
                task_id as "task_id!: String",
                parent_task_id as "parent_task_id: String",
                status as "status!: String",
                kind as "kind!: String",
                payload_json as "payload_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String",
                claimed_by as "claimed_by: String",
                attempts as "attempts!: i64",
                last_error as "last_error: String"
            FROM tasks
            WHERE (?1 IS NULL OR status = ?1)
            ORDER BY created_at DESC
            LIMIT ?2"#,
            status,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to query task list")?;

        rows.into_iter()
            .map(|row| {
                Ok(Task {
                    task_id: Uri::parse(&row.task_id)?,
                    parent_task_id: row.parent_task_id.as_deref().map(Uri::parse).transpose()?,
                    status: parse_task_status(&row.status)?,
                    kind: parse_task_kind(&row.kind)?,
                    payload: serde_json::from_str(&row.payload_json).unwrap_or(Value::Null),
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                    claimed_by: row.claimed_by.as_deref().map(Uri::parse).transpose()?,
                    attempts: row.attempts,
                    last_error: row.last_error,
                })
            })
            .collect()
    }

    pub async fn get_task(&self, task_id: &Uri) -> Result<Option<Task>> {
        let task_id = task_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                task_id as "task_id!: String",
                parent_task_id as "parent_task_id: String",
                status as "status!: String",
                kind as "kind!: String",
                payload_json as "payload_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String",
                claimed_by as "claimed_by: String",
                attempts as "attempts!: i64",
                last_error as "last_error: String"
            FROM tasks
            WHERE task_id = ?1"#,
            task_id,
        )
            .fetch_optional(self.conn.pool())
            .await
            .context("failed to query task")?;

        row.map(|row| {
            Ok(Task {
                task_id: Uri::parse(&row.task_id)?,
                parent_task_id: row.parent_task_id.as_deref().map(Uri::parse).transpose()?,
                status: parse_task_status(&row.status)?,
                kind: parse_task_kind(&row.kind)?,
                payload: serde_json::from_str(&row.payload_json).unwrap_or(Value::Null),
                created_at: parse_ts(&row.created_at)?,
                updated_at: parse_ts(&row.updated_at)?,
                claimed_by: row.claimed_by.as_deref().map(Uri::parse).transpose()?,
                attempts: row.attempts,
                last_error: row.last_error,
            })
        })
        .transpose()
    }

    pub async fn get_task_events(&self, task_id: &Uri) -> Result<Vec<TaskEvent>> {
        let task_id = task_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT
                event_id as "event_id!: String",
                task_id as "task_id!: String",
                ts as "ts!: String",
                type as "event_type!: String",
                payload_json as "payload_json!: String"
            FROM task_events
            WHERE task_id = ?1
            ORDER BY ts ASC"#,
            task_id,
        )
            .fetch_all(self.conn.pool())
            .await
            .context("failed to query task events")?;

        rows.into_iter()
            .map(|row| {
                Ok(TaskEvent {
                    event_id: Uri::parse(&row.event_id)?,
                    task_id: Uri::parse(&row.task_id)?,
                    ts: parse_ts(&row.ts)?,
                    event_type: Uri::parse(&row.event_type)?,
                    payload: serde_json::from_str(&row.payload_json).unwrap_or(Value::Null),
                })
            })
            .collect()
    }

    pub async fn claim_next_runnable_task(&self, worker_id: &Uri) -> Result<Option<Task>> {
        let worker_id = worker_id.to_string();
        let now = Utc::now().to_rfc3339();
        debug!(target: "borg_db", worker_id, "claiming next runnable task");

        let row = sqlx::query!(
            r#"SELECT
                t.task_id as "task_id!: String"
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
            LIMIT 1"#
        )
            .fetch_optional(self.conn.pool())
            .await
            .context("failed to query runnable task")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let task_id = row.task_id;
        let task_id_for_update = task_id.clone();
        let updated = sqlx::query!(
            "UPDATE tasks SET status = 'running', claimed_by = ?1, updated_at = ?2, attempts = attempts + 1 WHERE task_id = ?3 AND status = 'queued'",
            worker_id,
            now,
            task_id_for_update,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to claim task")?
            .rows_affected();

        if updated == 0 {
            return Ok(None);
        }

        self.get_task(&Uri::parse(&task_id)?).await
    }

    pub async fn requeue_running_tasks(&self) -> Result<u64> {
        info!(target: "borg_db", "requeueing running tasks");
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query!(
            "UPDATE tasks SET status = 'queued', claimed_by = NULL, updated_at = ?1 WHERE status = 'running'",
            now,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to requeue running tasks")?
            .rows_affected();
        Ok(updated)
    }

    pub async fn list_recoverable_task_ids(&self) -> Result<Vec<Uri>> {
        let rows = sqlx::query!(
            r#"SELECT
                task_id as "task_id!: String"
            FROM tasks
            WHERE status = 'queued'
            ORDER BY created_at ASC"#
        )
            .fetch_all(self.conn.pool())
            .await
            .context("failed to list recoverable task ids")?;

        rows.into_iter().map(|row| Uri::parse(&row.task_id)).collect()
    }

    pub async fn claim_task_by_id(&self, worker_id: &Uri, task_id: &Uri) -> Result<Option<Task>> {
        let worker_id = worker_id.to_string();
        let task_id = task_id.to_string();
        let now = Utc::now().to_rfc3339();
        debug!(target: "borg_db", worker_id, task_id, "claiming task by id");

        let task_id_for_claim_check = task_id.clone();
        let claimable = sqlx::query!(
            r#"SELECT
                t.task_id as "task_id!: String"
            FROM tasks t
            WHERE t.task_id = ?1
              AND t.status = 'queued'
              AND NOT EXISTS (
                SELECT 1
                FROM deps d
                JOIN tasks dep ON dep.task_id = d.depends_on_task_id
                WHERE d.task_id = ?1
                  AND dep.status != 'succeeded'
              )"#,
            task_id_for_claim_check,
        )
            .fetch_optional(self.conn.pool())
            .await
            .context("failed to query claimable task by id")?;

        if claimable.is_none() {
            return Ok(None);
        }

        let task_id_for_update = task_id.clone();
        let updated = sqlx::query!(
            "UPDATE tasks SET status = 'running', claimed_by = ?1, updated_at = ?2, attempts = attempts + 1 WHERE task_id = ?3 AND status = 'queued'",
            worker_id,
            now,
            task_id_for_update,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to claim task by id")?
            .rows_affected();

        if updated == 0 {
            return Ok(None);
        }

        self.get_task(&Uri::parse(&task_id)?).await
    }

    pub async fn complete_task(&self, task_id: &Uri, message: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let task_id_raw = task_id.to_string();
        let updated_at = now.clone();
        info!(target: "borg_db", task_id = %task_id, "marking task succeeded");

        sqlx::query!(
            "UPDATE tasks SET status = 'succeeded', updated_at = ?1, last_error = NULL WHERE task_id = ?2",
            updated_at,
            task_id_raw,
        )
            .execute(self.conn.pool())
            .await
            .context("failed updating task to succeeded")?;

        self.log_event(Event::TaskSucceeded {
            task_id: task_id.clone(),
            message: message.to_string(),
        })
        .await?;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &Uri, error_message: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let task_id_raw = task_id.to_string();
        let error_message_raw = error_message.to_string();
        let updated_at = now.clone();
        info!(target: "borg_db", task_id = %task_id, error = error_message, "marking task failed");

        sqlx::query!(
            "UPDATE tasks SET status = 'failed', updated_at = ?1, last_error = ?2 WHERE task_id = ?3",
            updated_at,
            error_message_raw,
            task_id_raw,
        )
            .execute(self.conn.pool())
            .await
            .context("failed updating task to failed")?;

        self.log_event(Event::TaskFailed {
            task_id: task_id.clone(),
            error: error_message.to_string(),
        })
        .await?;

        Ok(())
    }

    pub async fn log_event(&self, event: Event) -> Result<()> {
        let event_id = uri!("borg", "event");
        let task_id = event.task_id().to_string();
        let event_type = event.event_type().to_string();
        let payload = serde_json::to_string(&event)?;
        let now = Utc::now().to_rfc3339();
        let event_id = event_id.to_string();

        debug!(target: "borg_db", task_id, event_type, "writing task event");
        sqlx::query!(
            "INSERT INTO task_events(event_id, task_id, ts, type, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            event_id,
            task_id,
            now,
            event_type,
            payload,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to write task event")?;
        Ok(())
    }

    pub async fn add_dependency(&self, task_id: &Uri, depends_on_task_id: &Uri) -> Result<()> {
        let task_id = task_id.to_string();
        let depends_on_task_id = depends_on_task_id.to_string();
        sqlx::query!(
            "INSERT OR IGNORE INTO deps(task_id, depends_on_task_id) VALUES(?1, ?2)",
            task_id,
            depends_on_task_id,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to insert task dependency")?;
        Ok(())
    }
}
