use anyhow::{Context, Result};
use borg_core::{Uri, uri};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{ActorMailboxRecord, ActorRecord, BorgDb};

impl BorgDb {
    pub async fn upsert_actor(
        &self,
        actor_id: &Uri,
        name: &str,
        system_prompt: &str,
        status: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO actors(actor_id, name, system_prompt, status, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(actor_id) DO UPDATE SET
              name = excluded.name,
              system_prompt = excluded.system_prompt,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(actor_id.to_string())
        .bind(name)
        .bind(system_prompt)
        .bind(status)
        .bind(now.clone())
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to upsert actor")?;
        Ok(())
    }

    pub async fn get_actor(&self, actor_id: &Uri) -> Result<Option<ActorRecord>> {
        let row = sqlx::query(
            r#"
            SELECT actor_id, name, system_prompt, status, created_at, updated_at
            FROM actors
            WHERE actor_id = ?1
            LIMIT 1
            "#,
        )
        .bind(actor_id.to_string())
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get actor")?;

        row.map(actor_from_row).transpose()
    }

    pub async fn list_actors(&self, limit: usize) -> Result<Vec<ActorRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT actor_id, name, system_prompt, status, created_at, updated_at
            FROM actors
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list actors")?;

        rows.into_iter().map(actor_from_row).collect()
    }

    pub async fn delete_actor(&self, actor_id: &Uri) -> Result<u64> {
        let deleted = sqlx::query("DELETE FROM actors WHERE actor_id = ?1")
            .bind(actor_id.to_string())
            .execute(self.conn.pool())
            .await
            .context("failed to delete actor")?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn enqueue_actor_message(
        &self,
        actor_id: &Uri,
        kind: &str,
        session_id: Option<&Uri>,
        payload: &Value,
        reply_to_actor_id: Option<&Uri>,
        reply_to_message_id: Option<&Uri>,
    ) -> Result<Uri> {
        let actor_message_id = uri!("borg", "actor_message");
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO actor_mailbox(
                actor_message_id,
                actor_id,
                kind,
                session_id,
                payload_json,
                status,
                reply_to_actor_id,
                reply_to_message_id,
                created_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, 'QUEUED', ?6, ?7, ?8)
            "#,
        )
        .bind(actor_message_id.to_string())
        .bind(actor_id.to_string())
        .bind(kind)
        .bind(session_id.map(ToString::to_string))
        .bind(payload.to_string())
        .bind(reply_to_actor_id.map(ToString::to_string))
        .bind(reply_to_message_id.map(ToString::to_string))
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to enqueue actor mailbox message")?;

        Ok(actor_message_id)
    }

    pub async fn claim_next_actor_message(
        &self,
        actor_id: &Uri,
    ) -> Result<Option<ActorMailboxRecord>> {
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query(
            r#"
            WITH next AS (
                SELECT actor_message_id
                FROM actor_mailbox
                WHERE actor_id = ?1 AND status = 'QUEUED'
                ORDER BY created_at ASC
                LIMIT 1
            )
            UPDATE actor_mailbox
            SET status = 'IN_PROGRESS',
                started_at = ?2
            WHERE actor_message_id = (SELECT actor_message_id FROM next)
            RETURNING
                actor_message_id,
                actor_id,
                kind,
                session_id,
                payload_json,
                status,
                reply_to_actor_id,
                reply_to_message_id,
                error,
                created_at,
                started_at,
                finished_at
            "#,
        )
        .bind(actor_id.to_string())
        .bind(now)
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to claim next actor mailbox message")?;

        row.map(actor_mailbox_from_row).transpose()
    }

    pub async fn list_queued_actor_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<ActorMailboxRecord>> {
        let limit = i64::try_from(limit).unwrap_or(1_000);
        let rows = sqlx::query(
            r#"
            SELECT
                actor_message_id,
                actor_id,
                kind,
                session_id,
                payload_json,
                status,
                reply_to_actor_id,
                reply_to_message_id,
                error,
                created_at,
                started_at,
                finished_at
            FROM actor_mailbox
            WHERE status = 'QUEUED'
            ORDER BY created_at ASC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list queued actor mailbox messages")?;

        rows.into_iter().map(actor_mailbox_from_row).collect()
    }

    pub async fn ack_actor_message(&self, actor_message_id: &Uri) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query(
            r#"
            UPDATE actor_mailbox
            SET status = 'ACKED',
                finished_at = ?2,
                error = NULL
            WHERE actor_message_id = ?1
            "#,
        )
        .bind(actor_message_id.to_string())
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to ack actor mailbox message")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn fail_actor_message(&self, actor_message_id: &Uri, error: &str) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query(
            r#"
            UPDATE actor_mailbox
            SET status = 'FAILED',
                finished_at = ?2,
                error = ?3
            WHERE actor_message_id = ?1
            "#,
        )
        .bind(actor_message_id.to_string())
        .bind(now)
        .bind(error)
        .execute(self.conn.pool())
        .await
        .context("failed to fail actor mailbox message")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn fail_stale_in_progress_messages(&self, older_than_seconds: u64) -> Result<u64> {
        let cutoff = Utc::now()
            - chrono::Duration::seconds(i64::try_from(older_than_seconds).unwrap_or(300));
        let finished_at = Utc::now().to_rfc3339();
        let error = "failed due to runtime restart while in progress";
        let updated = sqlx::query(
            r#"
            UPDATE actor_mailbox
            SET status = 'FAILED',
                finished_at = ?1,
                error = ?2
            WHERE status = 'IN_PROGRESS'
              AND started_at IS NOT NULL
              AND started_at <= ?3
            "#,
        )
        .bind(finished_at)
        .bind(error)
        .bind(cutoff.to_rfc3339())
        .execute(self.conn.pool())
        .await
        .context("failed to fail stale in-progress actor mailbox messages")?
        .rows_affected();
        Ok(updated)
    }
}

fn actor_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ActorRecord> {
    Ok(ActorRecord {
        actor_id: Uri::parse(&row.try_get::<String, _>("actor_id")?)?,
        name: row.try_get("name")?,
        system_prompt: row.try_get("system_prompt")?,
        status: row.try_get("status")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

fn actor_mailbox_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ActorMailboxRecord> {
    let payload_raw: String = row.try_get("payload_json")?;
    let payload: Value =
        serde_json::from_str(&payload_raw).context("invalid actor_mailbox payload_json")?;
    let session_id = row
        .try_get::<Option<String>, _>("session_id")?
        .map(|value| Uri::parse(&value))
        .transpose()?;
    let reply_to_actor_id = row
        .try_get::<Option<String>, _>("reply_to_actor_id")?
        .map(|value| Uri::parse(&value))
        .transpose()?;
    let reply_to_message_id = row
        .try_get::<Option<String>, _>("reply_to_message_id")?
        .map(|value| Uri::parse(&value))
        .transpose()?;
    let started_at = row
        .try_get::<Option<String>, _>("started_at")?
        .as_deref()
        .map(parse_ts)
        .transpose()?;
    let finished_at = row
        .try_get::<Option<String>, _>("finished_at")?
        .as_deref()
        .map(parse_ts)
        .transpose()?;

    Ok(ActorMailboxRecord {
        actor_message_id: Uri::parse(&row.try_get::<String, _>("actor_message_id")?)?,
        actor_id: Uri::parse(&row.try_get::<String, _>("actor_id")?)?,
        kind: row.try_get("kind")?,
        session_id,
        payload,
        status: row.try_get("status")?,
        reply_to_actor_id,
        reply_to_message_id,
        error: row.try_get("error")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        started_at,
        finished_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "borg-db-actors-{test_name}-{}.db",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[tokio::test]
    async fn actor_mailbox_roundtrip_claim_ack() -> Result<()> {
        let path = tmp_db_path("claim_ack");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor_id = Uri::from_parts("devmode", "actor", Some("a1"))?;
        db.upsert_actor(&actor_id, "A1", "prompt", "RUNNING")
            .await?;

        let m1 = db
            .enqueue_actor_message(
                &actor_id,
                "CAST",
                None,
                &serde_json::json!({"n": 1}),
                None,
                None,
            )
            .await?;
        let _m2 = db
            .enqueue_actor_message(
                &actor_id,
                "CAST",
                None,
                &serde_json::json!({"n": 2}),
                None,
                None,
            )
            .await?;

        let claimed = db
            .claim_next_actor_message(&actor_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected claimed message"))?;
        assert_eq!(claimed.actor_message_id, m1);
        assert_eq!(claimed.status, "IN_PROGRESS");

        let acked = db.ack_actor_message(&claimed.actor_message_id).await?;
        assert_eq!(acked, 1);

        let next = db
            .claim_next_actor_message(&actor_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected second message"))?;
        assert_eq!(next.status, "IN_PROGRESS");

        Ok(())
    }

    #[tokio::test]
    async fn fail_stale_in_progress_marks_failed() -> Result<()> {
        let path = tmp_db_path("fail_stale");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor_id = Uri::from_parts("devmode", "actor", Some("a2"))?;
        db.upsert_actor(&actor_id, "A2", "prompt", "RUNNING")
            .await?;
        let msg_id = db
            .enqueue_actor_message(
                &actor_id,
                "CALL",
                None,
                &serde_json::json!({"task": "x"}),
                None,
                None,
            )
            .await?;

        let claimed = db
            .claim_next_actor_message(&actor_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected claimed"))?;
        assert_eq!(claimed.actor_message_id, msg_id);
        assert_eq!(claimed.status, "IN_PROGRESS");

        let failed = db.fail_stale_in_progress_messages(0).await?;
        assert_eq!(failed, 1);

        let row =
            sqlx::query("SELECT status FROM actor_mailbox WHERE actor_message_id = ?1 LIMIT 1")
                .bind(msg_id.to_string())
                .fetch_one(db.pool())
                .await?;
        let status: String = row.try_get("status")?;
        assert_eq!(status, "FAILED");

        Ok(())
    }
}
