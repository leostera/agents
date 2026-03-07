use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use borg_core::Uri;
use borg_db::BorgDb;
use chrono::Utc;
use sqlx::{Row, Sqlite, Transaction};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use uuid::Uuid;

use crate::model::{
    CommentRecord, EventRecord, ReviewState, TaskEventData, TaskRecord, TaskStatus,
};

#[derive(Debug, Clone)]
pub struct CreateTaskInput {
    pub title: String,
    pub description: String,
    pub definition_of_done: String,
    pub assignee_actor_id: String,
    pub parent_uri: Option<String>,
    pub blocked_by: Vec<String>,
    pub references: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub definition_of_done: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SplitSubtaskInput {
    pub title: String,
    pub description: String,
    pub definition_of_done: String,
    pub assignee_actor_id: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ListParams {
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
struct TaskNode {
    uri: String,
    status: TaskStatus,
    assignee_actor_id: String,
    parent_uri: Option<String>,
}

#[derive(Clone)]
pub struct TaskGraphStore {
    db: BorgDb,
}

impl TaskGraphStore {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &BorgDb {
        &self.db
    }

    pub async fn clear_all_tasks(&self) -> Result<u64> {
        let mut tx = self.db.pool().begin().await?;
        let deleted = sqlx::query("DELETE FROM taskgraph_tasks")
            .execute(tx.as_mut())
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(deleted)
    }

    pub async fn create_task(
        &self,
        actor_id: &str,
        creator_actor_id: &str,
        input: CreateTaskInput,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_non_empty(creator_actor_id, "task.validation_failed: creator_actor_id")?;
        ensure_non_empty(&input.title, "task.validation_failed: title")?;
        ensure_non_empty(
            &input.assignee_actor_id,
            "task.validation_failed: assignee_actor_id",
        )?;

        if let Some(parent_uri) = &input.parent_uri {
            ensure_uri(parent_uri, "task.invalid_uri")?;
        }
        for blocker in &input.blocked_by {
            ensure_uri(blocker, "task.invalid_uri")?;
        }
        for reference in &input.references {
            ensure_uri(reference, "task.invalid_uri")?;
        }
        for label in &input.labels {
            ensure_label(label)?;
        }

        ensure_actor_exists(
            &self.db,
            creator_actor_id,
            "task.validation_failed: creator_actor_id",
        )
        .await
        .ok();
        ensure_actor_exists(
            &self.db,
            &input.assignee_actor_id,
            "task.validation_failed: assignee_actor_id",
        )
        .await
        .ok();

        let task_uri = new_uri("task")?;
        let assignee_actor_id = input.assignee_actor_id.trim().to_string();
        let reviewer_actor_id = creator_actor_id.trim().to_string();
        let assignee_actor_id = assignee_actor_id.clone();
        let reviewer_actor_id = reviewer_actor_id.clone();
        let now = now_rfc3339();
        let mut tx = self.db.pool().begin().await?;

        if let Some(parent_uri) = &input.parent_uri {
            ensure_task_exists_tx(&mut tx, parent_uri).await?;
        }

        for blocker in &input.blocked_by {
            ensure_task_exists_tx(&mut tx, blocker).await?;
        }

        sqlx::query(
            r#"INSERT INTO taskgraph_tasks(
                uri,
                title,
                description,
                definition_of_done,
                status,
                assignee_actor_id,
                reviewer_actor_id,
                parent_uri,
                duplicate_of,
                review_submitted_at,
                review_approved_at,
                review_changes_requested_at,
                created_at,
                updated_at
            ) VALUES(?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, NULL, NULL, NULL, NULL, ?8, ?9)"#,
        )
        .bind(&task_uri)
        .bind(input.title.trim())
        .bind(input.description.trim())
        .bind(input.definition_of_done.trim())
        .bind(&assignee_actor_id)
        .bind(&reviewer_actor_id)
        .bind(input.parent_uri.as_deref())
        .bind(&now)
        .bind(&now)
        .execute(tx.as_mut())
        .await?;

        for label in input.labels {
            sqlx::query(
                "INSERT OR IGNORE INTO taskgraph_task_labels(task_uri, label, created_at) VALUES(?1, ?2, ?3)",
            )
            .bind(&task_uri)
            .bind(label)
            .bind(&now)
            .execute(tx.as_mut())
            .await?;
        }

        for blocker in input.blocked_by {
            sqlx::query(
                "INSERT OR IGNORE INTO taskgraph_task_blocked_by(task_uri, blocked_by_uri, created_at) VALUES(?1, ?2, ?3)",
            )
            .bind(&task_uri)
            .bind(blocker)
            .bind(&now)
            .execute(tx.as_mut())
            .await?;
        }

        for reference in input.references {
            sqlx::query(
                "INSERT OR IGNORE INTO taskgraph_task_references(task_uri, reference_uri, created_at) VALUES(?1, ?2, ?3)",
            )
            .bind(&task_uri)
            .bind(reference)
            .bind(&now)
            .execute(tx.as_mut())
            .await?;
        }

        validate_dag_tx(&mut tx).await?;

        append_event_tx(
            &mut tx,
            &task_uri,
            actor_id,
            "task.created",
            TaskEventData::TaskCreated {
                assignee_actor_id,
                reviewer_actor_id,
                parent_uri: None,
            },
        )
        .await?;

        let task = load_task_tx(&mut tx, &task_uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;

        tx.commit().await?;
        Ok(task)
    }

    pub async fn get_task(&self, uri: &str) -> Result<TaskRecord> {
        ensure_uri(uri, "task.invalid_uri")?;
        let mut tx = self.db.pool().begin().await?;
        let task = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn list_tasks(
        &self,
        params: ListParams,
    ) -> Result<(Vec<TaskRecord>, Option<String>)> {
        let limit = normalized_limit(params.limit);
        let mut tx = self.db.pool().begin().await?;

        let (cursor_ts, cursor_id) = decode_cursor(params.cursor.as_deref())?;
        let mut rows = if let (Some(ts), Some(id)) = (&cursor_ts, &cursor_id) {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   WHERE (created_at > ?1 OR (created_at = ?1 AND uri > ?2))
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?3"#,
            )
            .bind(ts)
            .bind(id)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        } else {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?1"#,
            )
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        };

        let mut next_cursor = None;
        if rows.len() > limit {
            if let Some(last) = rows.get(limit - 1) {
                let ts: String = last.get("created_at");
                let id: String = last.get("uri");
                next_cursor = Some(encode_cursor(&ts, &id));
            }
            rows.truncate(limit);
        }

        let mut tasks = Vec::with_capacity(rows.len());
        for row in rows {
            let uri: String = row.get("uri");
            if let Some(task) = load_task_tx(&mut tx, &uri).await? {
                tasks.push(task);
            }
        }

        tx.commit().await?;
        Ok((tasks, next_cursor))
    }

    pub async fn update_task_fields(
        &self,
        actor_id: &str,
        uri: &str,
        patch: TaskPatch,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        let before = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        let title = patch
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(before.title.as_str())
            .to_string();
        let description = patch
            .description
            .as_deref()
            .map(str::trim)
            .unwrap_or(before.description.as_str())
            .to_string();
        let definition_of_done = patch
            .definition_of_done
            .as_deref()
            .map(str::trim)
            .unwrap_or(before.definition_of_done.as_str())
            .to_string();

        let now = now_rfc3339();
        sqlx::query(
            "UPDATE taskgraph_tasks SET title = ?1, description = ?2, definition_of_done = ?3, updated_at = ?4 WHERE uri = ?5",
        )
        .bind(&title)
        .bind(&description)
        .bind(&definition_of_done)
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.updated",
            TaskEventData::TaskUpdated {
                title,
                description,
                definition_of_done,
            },
        )
        .await?;

        let task = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn reassign_assignee(
        &self,
        actor_id: &str,
        uri: &str,
        new_assignee_actor_id: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_non_empty(
            new_assignee_actor_id,
            "task.validation_failed: assignee_actor_id",
        )?;

        let mut tx = self.db.pool().begin().await?;
        let task = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        if task.reviewer_actor_id != actor_id {
            return Err(anyhow!("task.reassign_forbidden"));
        }

        let old_assignee_actor_id = task.assignee_actor_id.clone();
        let new_assignee_actor_id = new_assignee_actor_id.trim().to_string();
        let now = now_rfc3339();

        sqlx::query(
            r#"UPDATE taskgraph_tasks
               SET assignee_actor_id = ?1,
                   status = 'pending',
                   review_submitted_at = NULL,
                   review_approved_at = NULL,
                   review_changes_requested_at = NULL,
                   updated_at = ?2
               WHERE uri = ?3"#,
        )
        .bind(new_assignee_actor_id.trim())
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.reassigned",
            TaskEventData::TaskReassigned {
                old_assignee_actor_id,
                new_assignee_actor_id: new_assignee_actor_id.trim().to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn add_task_labels(
        &self,
        actor_id: &str,
        uri: &str,
        labels: &[String],
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        if labels.is_empty() {
            return Err(anyhow!("task.validation_failed: labels must not be empty"));
        }

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        let now = now_rfc3339();

        for label in labels {
            ensure_label(label)?;
            sqlx::query(
                "INSERT OR IGNORE INTO taskgraph_task_labels(task_uri, label, created_at) VALUES(?1, ?2, ?3)",
            )
            .bind(uri)
            .bind(label)
            .bind(&now)
            .execute(tx.as_mut())
            .await?;
        }

        sqlx::query("UPDATE taskgraph_tasks SET updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "label.added",
            TaskEventData::Labels {
                labels: labels.to_vec(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn remove_task_labels(
        &self,
        actor_id: &str,
        uri: &str,
        labels: &[String],
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        if labels.is_empty() {
            return Err(anyhow!("task.validation_failed: labels must not be empty"));
        }

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        let now = now_rfc3339();

        for label in labels {
            sqlx::query("DELETE FROM taskgraph_task_labels WHERE task_uri = ?1 AND label = ?2")
                .bind(uri)
                .bind(label)
                .execute(tx.as_mut())
                .await?;
        }

        sqlx::query("UPDATE taskgraph_tasks SET updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "label.removed",
            TaskEventData::Labels {
                labels: labels.to_vec(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn set_task_parent(
        &self,
        actor_id: &str,
        uri: &str,
        parent_uri: &str,
    ) -> Result<(TaskRecord, TaskRecord)> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(parent_uri, "task.invalid_uri")?;
        if uri == parent_uri {
            return Err(anyhow!(
                "task.validation_failed: self parent is not allowed"
            ));
        }

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        ensure_task_exists_tx(&mut tx, parent_uri).await?;

        let now = now_rfc3339();
        sqlx::query("UPDATE taskgraph_tasks SET parent_uri = ?1, updated_at = ?2 WHERE uri = ?3")
            .bind(parent_uri)
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        validate_dag_tx(&mut tx).await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.parent_set",
            TaskEventData::ParentSet {
                parent_uri: parent_uri.to_string(),
            },
        )
        .await?;

        let child = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        let parent = load_task_tx(&mut tx, parent_uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;

        tx.commit().await?;
        Ok((child, parent))
    }

    pub async fn clear_task_parent(&self, actor_id: &str, uri: &str) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        let now = now_rfc3339();
        sqlx::query("UPDATE taskgraph_tasks SET parent_uri = NULL, updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.parent_cleared",
            TaskEventData::Empty {},
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn list_task_children(
        &self,
        uri: &str,
        params: ListParams,
    ) -> Result<(Vec<TaskRecord>, Option<String>)> {
        ensure_uri(uri, "task.invalid_uri")?;
        let limit = normalized_limit(params.limit);
        let mut tx = self.db.pool().begin().await?;
        ensure_task_exists_tx(&mut tx, uri).await?;

        let (cursor_ts, cursor_id) = decode_cursor(params.cursor.as_deref())?;
        let mut rows = if let (Some(ts), Some(id)) = (&cursor_ts, &cursor_id) {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   WHERE parent_uri = ?1
                     AND (created_at > ?2 OR (created_at = ?2 AND uri > ?3))
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?4"#,
            )
            .bind(uri)
            .bind(ts)
            .bind(id)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        } else {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   WHERE parent_uri = ?1
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?2"#,
            )
            .bind(uri)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        };

        let mut next_cursor = None;
        if rows.len() > limit {
            if let Some(last) = rows.get(limit - 1) {
                let ts: String = last.get("created_at");
                let id: String = last.get("uri");
                next_cursor = Some(encode_cursor(&ts, &id));
            }
            rows.truncate(limit);
        }

        let mut children = Vec::with_capacity(rows.len());
        for row in rows {
            let child_uri: String = row.get("uri");
            if let Some(task) = load_task_tx(&mut tx, &child_uri).await? {
                children.push(task);
            }
        }

        tx.commit().await?;
        Ok((children, next_cursor))
    }

    pub async fn add_task_blocked_by(
        &self,
        actor_id: &str,
        uri: &str,
        blocked_by: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(blocked_by, "task.invalid_uri")?;
        if uri == blocked_by {
            return Err(anyhow!("task.cycle_detected: self blocked_by"));
        }

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        ensure_task_exists_tx(&mut tx, blocked_by).await?;

        let now = now_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO taskgraph_task_blocked_by(task_uri, blocked_by_uri, created_at) VALUES(?1, ?2, ?3)",
        )
        .bind(uri)
        .bind(blocked_by)
        .bind(&now)
        .execute(tx.as_mut())
        .await?;

        validate_dag_tx(&mut tx).await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "dep.added",
            TaskEventData::BlockedBy {
                blocked_by: blocked_by.to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn remove_task_blocked_by(
        &self,
        actor_id: &str,
        uri: &str,
        blocked_by: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(blocked_by, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        sqlx::query(
            "DELETE FROM taskgraph_task_blocked_by WHERE task_uri = ?1 AND blocked_by_uri = ?2",
        )
        .bind(uri)
        .bind(blocked_by)
        .execute(tx.as_mut())
        .await?;

        let now = now_rfc3339();
        sqlx::query("UPDATE taskgraph_tasks SET updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "dep.removed",
            TaskEventData::BlockedBy {
                blocked_by: blocked_by.to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn set_task_duplicate_of(
        &self,
        actor_id: &str,
        uri: &str,
        duplicate_of: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(duplicate_of, "task.invalid_uri")?;

        if uri == duplicate_of {
            return Err(anyhow!(
                "task.validation_failed: duplicate_of cannot self-reference"
            ));
        }

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        ensure_task_exists_tx(&mut tx, duplicate_of).await?;

        let now = now_rfc3339();
        sqlx::query("UPDATE taskgraph_tasks SET duplicate_of = ?1, updated_at = ?2 WHERE uri = ?3")
            .bind(duplicate_of)
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.duplicate_set",
            TaskEventData::DuplicateOf {
                duplicate_of: duplicate_of.to_string(),
            },
        )
        .await?;

        cascade_discard_tx(&mut tx, uri, actor_id, "status.discarded.duplicate").await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn clear_task_duplicate_of(&self, actor_id: &str, uri: &str) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        let now = now_rfc3339();
        sqlx::query(
            "UPDATE taskgraph_tasks SET duplicate_of = NULL, updated_at = ?1 WHERE uri = ?2",
        )
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.duplicate_cleared",
            TaskEventData::Empty {},
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn list_duplicated_by(
        &self,
        uri: &str,
        params: ListParams,
    ) -> Result<(Vec<TaskRecord>, Option<String>)> {
        ensure_uri(uri, "task.invalid_uri")?;
        let limit = normalized_limit(params.limit);

        let mut tx = self.db.pool().begin().await?;
        ensure_task_exists_tx(&mut tx, uri).await?;

        let (cursor_ts, cursor_id) = decode_cursor(params.cursor.as_deref())?;
        let mut rows = if let (Some(ts), Some(id)) = (&cursor_ts, &cursor_id) {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   WHERE duplicate_of = ?1
                     AND (created_at > ?2 OR (created_at = ?2 AND uri > ?3))
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?4"#,
            )
            .bind(uri)
            .bind(ts)
            .bind(id)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        } else {
            sqlx::query(
                r#"SELECT uri, created_at
                   FROM taskgraph_tasks
                   WHERE duplicate_of = ?1
                   ORDER BY created_at ASC, uri ASC
                   LIMIT ?2"#,
            )
            .bind(uri)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        };

        let mut next_cursor = None;
        if rows.len() > limit {
            if let Some(last) = rows.get(limit - 1) {
                let ts: String = last.get("created_at");
                let id: String = last.get("uri");
                next_cursor = Some(encode_cursor(&ts, &id));
            }
            rows.truncate(limit);
        }

        let mut tasks = Vec::with_capacity(rows.len());
        for row in rows {
            let task_uri: String = row.get("uri");
            if let Some(task) = load_task_tx(&mut tx, &task_uri).await? {
                tasks.push(task);
            }
        }

        tx.commit().await?;
        Ok((tasks, next_cursor))
    }

    pub async fn add_task_reference(
        &self,
        actor_id: &str,
        uri: &str,
        reference: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(reference, "task.invalid_uri")?;
        let mut tx = self.db.pool().begin().await?;

        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        ensure_task_exists_tx(&mut tx, reference).await?;
        let now = now_rfc3339();

        sqlx::query(
            "INSERT OR IGNORE INTO taskgraph_task_references(task_uri, reference_uri, created_at) VALUES(?1, ?2, ?3)",
        )
        .bind(uri)
        .bind(reference)
        .bind(&now)
        .execute(tx.as_mut())
        .await?;

        sqlx::query("UPDATE taskgraph_tasks SET updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "reference.added",
            TaskEventData::Reference {
                reference: reference.to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn remove_task_reference(
        &self,
        actor_id: &str,
        uri: &str,
        reference: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_uri(reference, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        sqlx::query(
            "DELETE FROM taskgraph_task_references WHERE task_uri = ?1 AND reference_uri = ?2",
        )
        .bind(uri)
        .bind(reference)
        .execute(tx.as_mut())
        .await?;

        let now = now_rfc3339();
        sqlx::query("UPDATE taskgraph_tasks SET updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "reference.removed",
            TaskEventData::Reference {
                reference: reference.to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn set_task_status(
        &self,
        actor_id: &str,
        uri: &str,
        status: TaskStatus,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        let task = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        match status {
            TaskStatus::Pending | TaskStatus::Doing => {
                if task.assignee_actor_id != actor_id {
                    return Err(anyhow!("auth.forbidden: assignee actor required"));
                }
                let now = now_rfc3339();
                sqlx::query(
                    "UPDATE taskgraph_tasks SET status = ?1, updated_at = ?2 WHERE uri = ?3",
                )
                .bind(status.as_str())
                .bind(&now)
                .bind(uri)
                .execute(tx.as_mut())
                .await?;
                append_event_tx(
                    &mut tx,
                    uri,
                    actor_id,
                    "status.changed",
                    TaskEventData::Status {
                        status: status.as_str().to_string(),
                    },
                )
                .await?;
            }
            TaskStatus::Discarded => {
                if task.reviewer_actor_id != actor_id {
                    return Err(anyhow!("auth.forbidden: reviewer actor required"));
                }
                cascade_discard_tx(&mut tx, uri, actor_id, "status.discarded").await?;
            }
            TaskStatus::Review | TaskStatus::Done => {
                return Err(anyhow!(
                    "task.validation_failed: use review tools for review/done transitions"
                ));
            }
        }

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;

        tx.commit().await?;
        Ok(out)
    }

    pub async fn submit_review(&self, actor_id: &str, uri: &str) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        let task = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        if task.assignee_actor_id != actor_id {
            return Err(anyhow!("review.actor_mismatch"));
        }

        let current =
            TaskStatus::parse(&task.status).ok_or_else(|| anyhow!("task.validation_failed"))?;
        if !matches!(current, TaskStatus::Pending | TaskStatus::Doing) {
            return Err(anyhow!(
                "task.validation_failed: submitReview allowed only from pending or doing"
            ));
        }

        let now = now_rfc3339();
        sqlx::query(
            "UPDATE taskgraph_tasks SET status = 'review', review_submitted_at = ?1, updated_at = ?2 WHERE uri = ?3",
        )
        .bind(&now)
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "review.submitted",
            TaskEventData::ReviewSubmitted {
                submitted_at: now.clone(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn approve_review(&self, actor_id: &str, uri: &str) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;

        let mut tx = self.db.pool().begin().await?;
        let task = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        if task.reviewer_actor_id != actor_id {
            return Err(anyhow!("review.actor_mismatch"));
        }

        if task.review.submitted_at.is_none() {
            return Err(anyhow!("task.validation_failed: missing submitted_at"));
        }

        ensure_children_complete_tx(&mut tx, uri).await?;

        let now = now_rfc3339();
        sqlx::query(
            r#"UPDATE taskgraph_tasks
               SET status = 'done',
                   review_approved_at = ?1,
                   review_changes_requested_at = NULL,
                   updated_at = ?2
               WHERE uri = ?3"#,
        )
        .bind(&now)
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "review.approved",
            TaskEventData::ReviewApproved {
                approved_at: now.clone(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn request_review_changes(
        &self,
        actor_id: &str,
        uri: &str,
        return_to: TaskStatus,
        note: &str,
    ) -> Result<TaskRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_non_empty(note, "review.note_required")?;
        if !matches!(return_to, TaskStatus::Pending | TaskStatus::Doing) {
            return Err(anyhow!(
                "task.validation_failed: return_to must be pending or doing"
            ));
        }

        let mut tx = self.db.pool().begin().await?;
        let task = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;

        if task.reviewer_actor_id != actor_id {
            return Err(anyhow!("review.actor_mismatch"));
        }

        let now = now_rfc3339();
        sqlx::query(
            r#"UPDATE taskgraph_tasks
               SET status = ?1,
                   review_changes_requested_at = ?2,
                   review_approved_at = NULL,
                   updated_at = ?3
               WHERE uri = ?4"#,
        )
        .bind(return_to.as_str())
        .bind(&now)
        .bind(&now)
        .bind(uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "review.changes_requested",
            TaskEventData::ReviewChangesRequested {
                changes_requested_at: now.clone(),
                return_to: return_to.as_str().to_string(),
                note: note.to_string(),
            },
        )
        .await?;

        let out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;
        tx.commit().await?;
        Ok(out)
    }

    pub async fn split_task_into_subtasks(
        &self,
        actor_id: &str,
        creator_actor_id: &str,
        uri: &str,
        subtasks: Vec<SplitSubtaskInput>,
    ) -> Result<(TaskRecord, Vec<TaskRecord>)> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(uri, "task.invalid_uri")?;
        ensure_non_empty(creator_actor_id, "task.validation_failed: creator_actor_id")?;
        if subtasks.is_empty() {
            return Err(anyhow!(
                "task.validation_failed: subtasks must not be empty"
            ));
        }

        let mut tx = self.db.pool().begin().await?;
        let parent = ensure_mutation_allowed_tx(&mut tx, uri, actor_id).await?;
        if TaskStatus::parse(&parent.status) == Some(TaskStatus::Done) {
            return Err(anyhow!(
                "task.validation_failed: done tasks cannot be split"
            ));
        }

        let now = now_rfc3339();
        let mut created = Vec::with_capacity(subtasks.len());

        for subtask in subtasks {
            ensure_non_empty(&subtask.title, "task.validation_failed: subtask title")?;
            ensure_non_empty(
                &subtask.assignee_actor_id,
                "task.validation_failed: subtask assignee_actor_id",
            )?;
            for label in &subtask.labels {
                ensure_label(label)?;
            }

            let task_uri = new_uri("task")?;
            let assignee_actor_id = subtask.assignee_actor_id.trim().to_string();
            let reviewer_actor_id = creator_actor_id.trim().to_string();
            let assignee_actor_id = assignee_actor_id.clone();
            let reviewer_actor_id = reviewer_actor_id.clone();

            sqlx::query(
                r#"INSERT INTO taskgraph_tasks(
                    uri,
                    title,
                    description,
                    definition_of_done,
                    status,
                    assignee_actor_id,
                    reviewer_actor_id,
                    parent_uri,
                    duplicate_of,
                    review_submitted_at,
                    review_approved_at,
                    review_changes_requested_at,
                    created_at,
                    updated_at
                ) VALUES(?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, NULL, NULL, NULL, NULL, ?8, ?9)"#,
            )
            .bind(&task_uri)
            .bind(subtask.title.trim())
            .bind(subtask.description.trim())
            .bind(subtask.definition_of_done.trim())
            .bind(&assignee_actor_id)
            .bind(&reviewer_actor_id)
            .bind(uri)
            .bind(&now)
            .bind(&now)
            .execute(tx.as_mut())
            .await?;

            for label in subtask.labels {
                sqlx::query(
                    "INSERT OR IGNORE INTO taskgraph_task_labels(task_uri, label, created_at) VALUES(?1, ?2, ?3)",
                )
                .bind(&task_uri)
                .bind(label)
                .bind(&now)
                .execute(tx.as_mut())
                .await?;
            }

            append_event_tx(
                &mut tx,
                &task_uri,
                actor_id,
                "task.created",
                TaskEventData::TaskCreated {
                    assignee_actor_id,
                    reviewer_actor_id,
                    parent_uri: Some(uri.to_string()),
                },
            )
            .await?;

            if let Some(created_task) = load_task_tx(&mut tx, &task_uri).await? {
                created.push(created_task);
            }
        }

        sqlx::query("UPDATE taskgraph_tasks SET status = 'doing', updated_at = ?1 WHERE uri = ?2")
            .bind(&now)
            .bind(uri)
            .execute(tx.as_mut())
            .await?;

        append_event_tx(
            &mut tx,
            uri,
            actor_id,
            "task.split",
            TaskEventData::TaskSplit {
                subtask_count: created.len() as i64,
            },
        )
        .await?;

        let parent_out = load_task_tx(&mut tx, uri)
            .await?
            .ok_or_else(|| anyhow!("task.not_found"))?;

        tx.commit().await?;
        Ok((parent_out, created))
    }

    pub async fn add_comment(
        &self,
        actor_id: &str,
        task_uri: &str,
        body: &str,
    ) -> Result<CommentRecord> {
        ensure_uri(actor_id, "auth.actor_required")?;
        ensure_uri(task_uri, "task.invalid_uri")?;
        ensure_non_empty(body, "task.validation_failed: comment body")?;

        let mut tx = self.db.pool().begin().await?;
        ensure_task_exists_tx(&mut tx, task_uri).await?;
        let id = Uuid::now_v7().to_string();
        let now = now_rfc3339();

        sqlx::query(
            "INSERT INTO taskgraph_comments(id, task_uri, author_actor_id, body, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&id)
        .bind(task_uri)
        .bind(actor_id)
        .bind(body.trim())
        .bind(&now)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            &mut tx,
            task_uri,
            actor_id,
            "comment.added",
            TaskEventData::CommentAdded {
                comment_id: id.clone(),
            },
        )
        .await?;

        tx.commit().await?;
        Ok(CommentRecord {
            id,
            task_uri: task_uri.to_string(),
            author_actor_id: actor_id.to_string(),
            body: body.trim().to_string(),
            created_at: now,
        })
    }

    pub async fn list_comments(
        &self,
        task_uri: &str,
        params: ListParams,
    ) -> Result<(Vec<CommentRecord>, Option<String>)> {
        ensure_uri(task_uri, "task.invalid_uri")?;
        let limit = normalized_limit(params.limit);
        let mut tx = self.db.pool().begin().await?;
        ensure_task_exists_tx(&mut tx, task_uri).await?;

        let (cursor_ts, cursor_id) = decode_cursor(params.cursor.as_deref())?;
        let mut rows = if let (Some(ts), Some(id)) = (&cursor_ts, &cursor_id) {
            sqlx::query(
                r#"SELECT id, task_uri, author_actor_id, body, created_at
                   FROM taskgraph_comments
                   WHERE task_uri = ?1
                     AND (created_at > ?2 OR (created_at = ?2 AND id > ?3))
                   ORDER BY created_at ASC, id ASC
                   LIMIT ?4"#,
            )
            .bind(task_uri)
            .bind(ts)
            .bind(id)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, task_uri, author_actor_id, body, created_at
                   FROM taskgraph_comments
                   WHERE task_uri = ?1
                   ORDER BY created_at ASC, id ASC
                   LIMIT ?2"#,
            )
            .bind(task_uri)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        };

        let mut next_cursor = None;
        if rows.len() > limit {
            if let Some(last) = rows.get(limit - 1) {
                let ts: String = last.get("created_at");
                let id: String = last.get("id");
                next_cursor = Some(encode_cursor(&ts, &id));
            }
            rows.truncate(limit);
        }

        let comments = rows
            .into_iter()
            .map(|row| CommentRecord {
                id: row.get("id"),
                task_uri: row.get("task_uri"),
                author_actor_id: row.get("author_actor_id"),
                body: row.get("body"),
                created_at: row.get("created_at"),
            })
            .collect();

        tx.commit().await?;
        Ok((comments, next_cursor))
    }

    pub async fn list_events(
        &self,
        task_uri: &str,
        params: ListParams,
    ) -> Result<(Vec<EventRecord>, Option<String>)> {
        ensure_uri(task_uri, "task.invalid_uri")?;
        let limit = normalized_limit(params.limit);
        let mut tx = self.db.pool().begin().await?;
        ensure_task_exists_tx(&mut tx, task_uri).await?;

        let (cursor_ts, cursor_id) = decode_cursor(params.cursor.as_deref())?;
        let mut rows = if let (Some(ts), Some(id)) = (&cursor_ts, &cursor_id) {
            sqlx::query(
                r#"SELECT id, task_uri, actor_id, event_type, data_json, created_at
                   FROM taskgraph_events
                   WHERE task_uri = ?1
                     AND (created_at > ?2 OR (created_at = ?2 AND id > ?3))
                   ORDER BY created_at ASC, id ASC
                   LIMIT ?4"#,
            )
            .bind(task_uri)
            .bind(ts)
            .bind(id)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, task_uri, actor_id, event_type, data_json, created_at
                   FROM taskgraph_events
                   WHERE task_uri = ?1
                   ORDER BY created_at ASC, id ASC
                   LIMIT ?2"#,
            )
            .bind(task_uri)
            .bind((limit + 1) as i64)
            .fetch_all(tx.as_mut())
            .await?
        };

        let mut next_cursor = None;
        if rows.len() > limit {
            if let Some(last) = rows.get(limit - 1) {
                let ts: String = last.get("created_at");
                let id: String = last.get("id");
                next_cursor = Some(encode_cursor(&ts, &id));
            }
            rows.truncate(limit);
        }

        let events = rows
            .into_iter()
            .map(|row| EventRecord {
                id: row.get("id"),
                task_uri: row.get("task_uri"),
                actor_id: row.get("actor_id"),
                event_type: row.get("event_type"),
                data: parse_event_data_or_empty(row.get("data_json")),
                created_at: row.get("created_at"),
            })
            .collect();

        tx.commit().await?;
        Ok((events, next_cursor))
    }

    pub async fn next_task(&self, actor_id: &str, limit: usize) -> Result<Vec<TaskRecord>> {
        ensure_uri(actor_id, "auth.actor_required")?;
        let limit = normalized_limit(limit);
        let mut tx = self.db.pool().begin().await?;
        let tasks = load_all_task_nodes_tx(&mut tx).await?;
        let blockers = blockers_map_tx(&mut tx, &tasks).await?;
        let topo = topo_order(&tasks, &blockers);
        let by_uri: BTreeMap<String, TaskNode> = tasks
            .into_iter()
            .map(|task| (task.uri.clone(), task))
            .collect();

        let mut out = Vec::new();
        for task_uri in topo {
            let Some(task) = by_uri.get(&task_uri) else {
                continue;
            };
            if task.assignee_actor_id != actor_id {
                continue;
            }
            if !matches!(task.status, TaskStatus::Pending | TaskStatus::Doing) {
                continue;
            }
            if !is_task_eligible(&task.uri, &by_uri, &blockers) {
                continue;
            }
            if let Some(full) = load_task_tx(&mut tx, &task.uri).await? {
                out.push(full);
            }
            if out.len() >= limit {
                break;
            }
        }
        tx.commit().await?;
        Ok(out)
    }

    pub async fn reconcile_in_progress(
        &self,
        actor_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskRecord>> {
        ensure_uri(actor_id, "auth.actor_required")?;
        let limit = normalized_limit(limit);
        let mut tx = self.db.pool().begin().await?;
        let tasks = load_all_task_nodes_tx(&mut tx).await?;
        let blockers = blockers_map_tx(&mut tx, &tasks).await?;
        let topo = topo_order(&tasks, &blockers);
        let by_uri: BTreeMap<String, TaskNode> = tasks
            .into_iter()
            .map(|task| (task.uri.clone(), task))
            .collect();

        let mut out = Vec::new();
        for task_uri in topo {
            let Some(task) = by_uri.get(&task_uri) else {
                continue;
            };
            if task.assignee_actor_id != actor_id {
                continue;
            }
            if task.status != TaskStatus::Doing {
                continue;
            }
            if !is_task_eligible(&task.uri, &by_uri, &blockers) {
                continue;
            }
            if let Some(full) = load_task_tx(&mut tx, &task.uri).await? {
                out.push(full);
            }
            if out.len() >= limit {
                break;
            }
        }

        tx.commit().await?;
        Ok(out)
    }
}

fn normalized_limit(limit: usize) -> usize {
    limit.clamp(1, 100)
}

fn parse_event_data_or_empty(raw: String) -> TaskEventData {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn ensure_non_empty(input: &str, code: &str) -> Result<()> {
    if input.trim().is_empty() {
        return Err(anyhow!(code.to_string()));
    }
    Ok(())
}

async fn ensure_actor_exists(db: &BorgDb, actor_id: &str, code: &str) -> Result<()> {
    let uri = Uri::parse(actor_id).map_err(|_| anyhow!(code.to_string()))?;
    let actor = db
        .get_actor(&uri)
        .await
        .map_err(|_| anyhow!(code.to_string()))?;
    if actor.is_none() {
        return Err(anyhow!(code.to_string()));
    }
    Ok(())
}

fn ensure_label(label: &str) -> Result<()> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("task.validation_failed: empty label"));
    }
    let mut parts = trimmed.splitn(2, ':');
    let left = parts.next().unwrap_or_default().trim();
    let right = parts.next().unwrap_or_default().trim();
    if left.is_empty() || right.is_empty() {
        return Err(anyhow!(
            "task.validation_failed: labels must match scheme:value"
        ));
    }
    Ok(())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn new_uri(kind: &str) -> Result<String> {
    Ok(Uri::from_parts("borg", kind, Some(&Uuid::now_v7().to_string()))?.to_string())
}

fn ensure_uri(uri: &str, code: &str) -> Result<()> {
    Uri::parse(uri)
        .map(|_| ())
        .map_err(|_| anyhow!(code.to_string()))
}

fn encode_cursor(ts: &str, id: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}|{}", ts, id))
}

fn decode_cursor(cursor: Option<&str>) -> Result<(Option<String>, Option<String>)> {
    let Some(cursor) = cursor else {
        return Ok((None, None));
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| anyhow!("task.validation_failed: invalid cursor"))?;
    let decoded =
        String::from_utf8(bytes).map_err(|_| anyhow!("task.validation_failed: invalid cursor"))?;
    let mut parts = decoded.splitn(2, '|');
    let ts = parts.next().unwrap_or_default().to_string();
    let id = parts.next().unwrap_or_default().to_string();
    if ts.is_empty() || id.is_empty() {
        return Err(anyhow!("task.validation_failed: invalid cursor"));
    }
    Ok((Some(ts), Some(id)))
}

async fn ensure_task_exists_tx(tx: &mut Transaction<'_, Sqlite>, uri: &str) -> Result<()> {
    let row = sqlx::query("SELECT uri FROM taskgraph_tasks WHERE uri = ?1 LIMIT 1")
        .bind(uri)
        .fetch_optional(tx.as_mut())
        .await?;
    if row.is_none() {
        return Err(anyhow!("task.not_found"));
    }
    Ok(())
}

async fn ensure_mutation_allowed_tx(
    tx: &mut Transaction<'_, Sqlite>,
    uri: &str,
    actor_id: &str,
) -> Result<TaskRecord> {
    let task = load_task_tx(tx, uri)
        .await?
        .ok_or_else(|| anyhow!("task.not_found"))?;
    if task.assignee_actor_id != actor_id && task.reviewer_actor_id != actor_id {
        return Err(anyhow!("auth.forbidden"));
    }
    Ok(task)
}

async fn load_task_tx(tx: &mut Transaction<'_, Sqlite>, uri: &str) -> Result<Option<TaskRecord>> {
    let row = sqlx::query(
        r#"SELECT
            uri,
            title,
            description,
            definition_of_done,
            status,
            assignee_actor_id,
            reviewer_actor_id,
            parent_uri,
            duplicate_of,
            review_submitted_at,
            review_approved_at,
            review_changes_requested_at,
            created_at,
            updated_at
        FROM taskgraph_tasks
        WHERE uri = ?1
        LIMIT 1"#,
    )
    .bind(uri)
    .fetch_optional(tx.as_mut())
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let labels = sqlx::query(
        r#"SELECT label
           FROM taskgraph_task_labels
           WHERE task_uri = ?1
           ORDER BY label ASC"#,
    )
    .bind(uri)
    .fetch_all(tx.as_mut())
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("label"))
    .collect();

    let blocked_by = sqlx::query(
        r#"SELECT blocked_by_uri
           FROM taskgraph_task_blocked_by
           WHERE task_uri = ?1
           ORDER BY created_at ASC, blocked_by_uri ASC"#,
    )
    .bind(uri)
    .fetch_all(tx.as_mut())
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("blocked_by_uri"))
    .collect();

    let references = sqlx::query(
        r#"SELECT reference_uri
           FROM taskgraph_task_references
           WHERE task_uri = ?1
           ORDER BY created_at ASC, reference_uri ASC"#,
    )
    .bind(uri)
    .fetch_all(tx.as_mut())
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("reference_uri"))
    .collect();

    Ok(Some(TaskRecord {
        uri: row.get("uri"),
        title: row.get("title"),
        description: row.get("description"),
        definition_of_done: row.get("definition_of_done"),
        status: row.get("status"),
        assignee_actor_id: row.get("assignee_actor_id"),
        reviewer_actor_id: row.get("reviewer_actor_id"),
        labels,
        parent_uri: row.get("parent_uri"),
        blocked_by,
        duplicate_of: row.get("duplicate_of"),
        references,
        review: ReviewState {
            submitted_at: row.get("review_submitted_at"),
            approved_at: row.get("review_approved_at"),
            changes_requested_at: row.get("review_changes_requested_at"),
        },
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

async fn append_event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    task_uri: &str,
    actor_id: &str,
    event_type: &str,
    data: TaskEventData,
) -> Result<()> {
    let id = Uuid::now_v7().to_string();
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO taskgraph_events(id, task_uri, actor_id, event_type, data_json, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(id)
    .bind(task_uri)
    .bind(actor_id)
    .bind(event_type)
    .bind(serde_json::to_string(&data)?)
    .bind(now)
    .execute(tx.as_mut())
    .await?;
    Ok(())
}

async fn load_all_task_nodes_tx(tx: &mut Transaction<'_, Sqlite>) -> Result<Vec<TaskNode>> {
    let rows = sqlx::query(
        r#"SELECT uri, status, assignee_actor_id, parent_uri
           FROM taskgraph_tasks"#,
    )
    .fetch_all(tx.as_mut())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status_raw: String = row.get("status");
        let Some(status) = TaskStatus::parse(&status_raw) else {
            continue;
        };
        out.push(TaskNode {
            uri: row.get("uri"),
            status,
            assignee_actor_id: row.get("assignee_actor_id"),
            parent_uri: row.get("parent_uri"),
        });
    }
    Ok(out)
}

async fn blockers_map_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tasks: &[TaskNode],
) -> Result<BTreeMap<String, Vec<String>>> {
    let node_set: BTreeSet<String> = tasks.iter().map(|task| task.uri.clone()).collect();
    let mut blockers: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for task in tasks {
        blockers.entry(task.uri.clone()).or_default();
    }

    let rows = sqlx::query("SELECT task_uri, blocked_by_uri FROM taskgraph_task_blocked_by")
        .fetch_all(tx.as_mut())
        .await?;
    for row in rows {
        let task_uri: String = row.get("task_uri");
        let blocked_by_uri: String = row.get("blocked_by_uri");
        if node_set.contains(&task_uri) && node_set.contains(&blocked_by_uri) {
            blockers.entry(task_uri).or_default().push(blocked_by_uri);
        }
    }

    for task in tasks {
        if let Some(parent_uri) = &task.parent_uri
            && node_set.contains(parent_uri)
        {
            blockers
                .entry(parent_uri.clone())
                .or_default()
                .push(task.uri.clone());
        }
    }

    for values in blockers.values_mut() {
        values.sort();
        values.dedup();
    }

    Ok(blockers)
}

fn is_task_eligible(
    task_uri: &str,
    by_uri: &BTreeMap<String, TaskNode>,
    blockers_map: &BTreeMap<String, Vec<String>>,
) -> bool {
    let Some(blockers) = blockers_map.get(task_uri) else {
        return true;
    };
    blockers.iter().all(|blocker_uri| {
        by_uri
            .get(blocker_uri)
            .map(|task| task.status.is_complete())
            .unwrap_or(false)
    })
}

fn topo_order(tasks: &[TaskNode], blockers_map: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let nodes: BTreeSet<String> = tasks
        .iter()
        .filter(|task| task.status != TaskStatus::Discarded)
        .map(|task| task.uri.clone())
        .collect();

    let mut indegree: BTreeMap<String, usize> = nodes.iter().map(|uri| (uri.clone(), 0)).collect();
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (task_uri, blockers) in blockers_map {
        if !nodes.contains(task_uri) {
            continue;
        }
        for blocker in blockers {
            if !nodes.contains(blocker) {
                continue;
            }
            *indegree.entry(task_uri.clone()).or_default() += 1;
            adjacency
                .entry(blocker.clone())
                .or_default()
                .push(task_uri.clone());
        }
    }

    for neighbors in adjacency.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter_map(|(uri, degree)| {
            if *degree == 0 {
                Some(uri.clone())
            } else {
                None
            }
        })
        .collect();

    let mut out = Vec::with_capacity(nodes.len());
    while let Some(uri) = queue.pop_front() {
        out.push(uri.clone());
        if let Some(neighbors) = adjacency.get(&uri) {
            for neighbor in neighbors {
                if let Some(entry) = indegree.get_mut(neighbor) {
                    *entry = entry.saturating_sub(1);
                    if *entry == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if out.len() < nodes.len() {
        let processed: BTreeSet<String> = out.iter().cloned().collect();
        for node in nodes {
            if !processed.contains(&node) {
                out.push(node);
            }
        }
    }

    out
}

async fn ensure_children_complete_tx(tx: &mut Transaction<'_, Sqlite>, uri: &str) -> Result<()> {
    let rows = sqlx::query("SELECT status FROM taskgraph_tasks WHERE parent_uri = ?1")
        .bind(uri)
        .fetch_all(tx.as_mut())
        .await?;

    for row in rows {
        let status_raw: String = row.get("status");
        let status =
            TaskStatus::parse(&status_raw).ok_or_else(|| anyhow!("task.validation_failed"))?;
        if !status.is_complete() {
            return Err(anyhow!("task.children_incomplete"));
        }
    }
    Ok(())
}

async fn collect_descendants_tx(
    tx: &mut Transaction<'_, Sqlite>,
    root_uri: &str,
) -> Result<Vec<String>> {
    let mut queue = VecDeque::from([root_uri.to_string()]);
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }
        ordered.push(current.clone());

        let rows = sqlx::query("SELECT uri FROM taskgraph_tasks WHERE parent_uri = ?1")
            .bind(&current)
            .fetch_all(tx.as_mut())
            .await?;
        for row in rows {
            let child: String = row.get("uri");
            queue.push_back(child);
        }
    }

    Ok(ordered)
}

async fn cascade_discard_tx(
    tx: &mut Transaction<'_, Sqlite>,
    root_uri: &str,
    actor_id: &str,
    event_type: &str,
) -> Result<()> {
    let descendants = collect_descendants_tx(tx, root_uri).await?;
    let now = now_rfc3339();

    for uri in descendants {
        let current_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM taskgraph_tasks WHERE uri = ?1 LIMIT 1")
                .bind(&uri)
                .fetch_optional(tx.as_mut())
                .await?;

        let Some(current_status) = current_status else {
            continue;
        };

        if current_status == TaskStatus::Discarded.as_str() {
            continue;
        }

        sqlx::query(
            "UPDATE taskgraph_tasks SET status = 'discarded', updated_at = ?1 WHERE uri = ?2",
        )
        .bind(&now)
        .bind(&uri)
        .execute(tx.as_mut())
        .await?;

        append_event_tx(
            tx,
            &uri,
            actor_id,
            event_type,
            TaskEventData::Status {
                status: "discarded".to_string(),
            },
        )
        .await?;
    }

    Ok(())
}

async fn validate_dag_tx(tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
    let tasks = load_all_task_nodes_tx(tx).await?;
    let node_set: BTreeSet<String> = tasks.iter().map(|task| task.uri.clone()).collect();

    let blocked_rows =
        sqlx::query("SELECT task_uri, blocked_by_uri FROM taskgraph_task_blocked_by")
            .fetch_all(tx.as_mut())
            .await?;

    let mut indegree: BTreeMap<String, usize> =
        node_set.iter().map(|uri| (uri.clone(), 0)).collect();
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for row in blocked_rows {
        let from: String = row.get("task_uri");
        let to: String = row.get("blocked_by_uri");
        if !node_set.contains(&from) || !node_set.contains(&to) {
            continue;
        }
        if from == to {
            return Err(anyhow!("task.cycle_detected"));
        }
        *indegree.entry(to.clone()).or_default() += 1;
        adjacency.entry(from).or_default().push(to);
    }

    for task in &tasks {
        if let Some(parent_uri) = &task.parent_uri {
            if !node_set.contains(parent_uri) {
                continue;
            }
            let from = parent_uri.clone();
            let to = task.uri.clone();
            if from == to {
                return Err(anyhow!("task.cycle_detected"));
            }
            *indegree.entry(to.clone()).or_default() += 1;
            adjacency.entry(from).or_default().push(to);
        }
    }

    for neighbors in adjacency.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter_map(|(uri, degree)| {
            if *degree == 0 {
                Some(uri.clone())
            } else {
                None
            }
        })
        .collect();

    let mut processed = 0usize;
    while let Some(node) = queue.pop_front() {
        processed += 1;
        if let Some(neighbors) = adjacency.get(&node) {
            for neighbor in neighbors {
                if let Some(entry) = indegree.get_mut(neighbor) {
                    *entry = entry.saturating_sub(1);
                    if *entry == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if processed != node_set.len() {
        return Err(anyhow!("task.cycle_detected"));
    }

    Ok(())
}
