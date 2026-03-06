use anyhow::{Context, Result};
use borg_core::{Uri, uri};
use chrono::Utc;
use serde_json::Value;

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
        let actor_id = actor_id.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO actors(
                actor_id,
                name,
                model,
                default_provider_id,
                system_prompt,
                status,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)
            ON CONFLICT(actor_id) DO UPDATE SET
              name = excluded.name,
              system_prompt = excluded.system_prompt,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
            actor_id,
            name,
            Option::<String>::None,
            system_prompt,
            status,
            now,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to upsert actor")?;
        Ok(())
    }

    pub async fn get_actor(&self, actor_id: &Uri) -> Result<Option<ActorRecord>> {
        let actor_id = actor_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                actor_id as "actor_id!: String",
                name as "name!: String",
                model as "model: String",
                default_provider_id as "default_provider_id: String",
                system_prompt as "system_prompt!: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM actors
            WHERE actor_id = ?1
            LIMIT 1
            "#,
            actor_id,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get actor")?;

        row.map(|row| {
            Ok(ActorRecord {
                actor_id: Uri::parse(&row.actor_id)?,
                name: row.name,
                model: row.model,
                default_provider_id: row.default_provider_id,
                system_prompt: row.system_prompt,
                status: row.status,
                created_at: parse_ts(&row.created_at)?,
                updated_at: parse_ts(&row.updated_at)?,
            })
        })
        .transpose()
    }

    pub async fn list_actors(&self, limit: usize) -> Result<Vec<ActorRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"
            SELECT
                actor_id as "actor_id!: String",
                name as "name!: String",
                model as "model: String",
                default_provider_id as "default_provider_id: String",
                system_prompt as "system_prompt!: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM actors
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list actors")?;

        rows.into_iter()
            .map(|row| {
                Ok(ActorRecord {
                    actor_id: Uri::parse(&row.actor_id)?,
                    name: row.name,
                    model: row.model,
                    default_provider_id: row.default_provider_id,
                    system_prompt: row.system_prompt,
                    status: row.status,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn set_actor_model(&self, actor_id: &Uri, model: &str) -> Result<u64> {
        let actor_id = actor_id.to_string();
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query!(
            r#"
            UPDATE actors
            SET model = ?2,
                updated_at = ?3
            WHERE actor_id = ?1
            "#,
            actor_id,
            model,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to update actor model")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn delete_actor(&self, actor_id: &Uri) -> Result<u64> {
        let actor_id = actor_id.to_string();
        let deleted = sqlx::query!("DELETE FROM actors WHERE actor_id = ?1", actor_id,)
            .execute(self.conn.pool())
            .await
            .context("failed to delete actor")?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn list_actor_sessions(&self, actor_id: &Uri, limit: usize) -> Result<Vec<Uri>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let actor_id = actor_id.to_string();
        let rows = sqlx::query!(
            r#"
            SELECT session_id as "session_id: String"
            FROM messages
            WHERE receiver_id = ?1
              AND session_id IS NOT NULL
            GROUP BY session_id
            ORDER BY MAX(created_at) DESC
            LIMIT ?2
            "#,
            actor_id,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list actor sessions")?;

        rows.into_iter()
            .filter_map(|row| row.session_id)
            .map(|value| Uri::parse(&value))
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn enqueue_actor_message(
        &self,
        actor_id: &Uri,
        session_id: Option<&Uri>,
        payload: &Value,
        reply_to_actor_id: Option<&Uri>,
        reply_to_message_id: Option<&Uri>,
    ) -> Result<Uri> {
        self.enqueue_actor_message_from_sender(
            None,
            actor_id,
            session_id,
            payload,
            reply_to_actor_id,
            reply_to_message_id,
        )
        .await
    }

    pub async fn enqueue_actor_message_from_sender(
        &self,
        sender_actor_id: Option<&Uri>,
        actor_id: &Uri,
        session_id: Option<&Uri>,
        payload: &Value,
        reply_to_actor_id: Option<&Uri>,
        reply_to_message_id: Option<&Uri>,
    ) -> Result<Uri> {
        let actor_message_id = uri!("borg", "actor_message");
        let actor_message_id_raw = actor_message_id.to_string();
        let sender_actor_id = sender_actor_id.map(ToString::to_string);
        let actor_id = actor_id.to_string();
        let session_id = session_id.map(ToString::to_string);
        let payload_json = payload.to_string();
        let reply_to_actor_id = reply_to_actor_id.map(ToString::to_string);
        let reply_to_message_id = reply_to_message_id.map(ToString::to_string);
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO messages(
                message_id,
                sender_id,
                receiver_id,
                session_id,
                payload_json,
                status,
                reply_to_sender_id,
                reply_to_message_id,
                error,
                created_at,
                started_at,
                finished_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, 'QUEUED', ?6, ?7, NULL, ?8, NULL, NULL)
            "#,
            actor_message_id_raw,
            sender_actor_id,
            actor_id,
            session_id,
            payload_json,
            reply_to_actor_id,
            reply_to_message_id,
            now,
        )
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
        let actor_id = actor_id.to_string();
        let row = sqlx::query!(
            r#"
            WITH next AS (
                SELECT message_id
                FROM messages
                WHERE receiver_id = ?1
                  AND status = 'QUEUED'
                  AND reply_to_message_id IS NULL
                ORDER BY created_at ASC
                LIMIT 1
            )
            UPDATE messages
            SET status = 'IN_PROGRESS',
                started_at = ?2
            WHERE message_id = (SELECT message_id FROM next)
            RETURNING
                message_id as "actor_message_id!: String",
                sender_id as "sender_actor_id: String",
                receiver_id as "actor_id!: String",
                session_id as "session_id: String",
                payload_json as "payload_json!: String",
                status as "status!: String",
                reply_to_sender_id as "reply_to_actor_id: String",
                reply_to_message_id as "reply_to_message_id: String",
                error as "error: String",
                created_at as "created_at!: String",
                started_at as "started_at: String",
                finished_at as "finished_at: String"
            "#,
            actor_id,
            now,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to claim next actor mailbox message")?;

        row.map(|row| {
            actor_mailbox_from_raw(
                row.actor_message_id,
                row.sender_actor_id,
                row.actor_id,
                row.session_id,
                row.payload_json,
                row.status,
                row.reply_to_actor_id,
                row.reply_to_message_id,
                row.error,
                row.created_at,
                row.started_at,
                row.finished_at,
            )
        })
        .transpose()
    }

    pub async fn list_queued_actor_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<ActorMailboxRecord>> {
        let limit = i64::try_from(limit).unwrap_or(1_000);
        let rows = sqlx::query!(
            r#"
            SELECT
                message_id as "actor_message_id!: String",
                sender_id as "sender_actor_id: String",
                receiver_id as "actor_id!: String",
                session_id as "session_id: String",
                payload_json as "payload_json!: String",
                status as "status!: String",
                reply_to_sender_id as "reply_to_actor_id: String",
                reply_to_message_id as "reply_to_message_id: String",
                error as "error: String",
                created_at as "created_at!: String",
                started_at as "started_at: String",
                finished_at as "finished_at: String"
            FROM messages
            WHERE status = 'QUEUED'
              AND reply_to_message_id IS NULL
            ORDER BY created_at ASC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list queued actor mailbox messages")?;

        rows.into_iter()
            .map(|row| {
                actor_mailbox_from_raw(
                    row.actor_message_id,
                    row.sender_actor_id,
                    row.actor_id,
                    row.session_id,
                    row.payload_json,
                    row.status,
                    row.reply_to_actor_id,
                    row.reply_to_message_id,
                    row.error,
                    row.created_at,
                    row.started_at,
                    row.finished_at,
                )
            })
            .collect()
    }

    pub async fn claim_next_actor_reply_message(
        &self,
        actor_id: &Uri,
        session_id: &Uri,
        expected_reply_to_message_id: Option<&Uri>,
    ) -> Result<Option<ActorMailboxRecord>> {
        let now = Utc::now().to_rfc3339();
        let actor_id = actor_id.to_string();
        let session_id = session_id.to_string();
        let expected_reply_to_message_id = expected_reply_to_message_id.map(ToString::to_string);
        let row = sqlx::query!(
            r#"
            WITH next AS (
                SELECT message_id
                FROM messages
                WHERE receiver_id = ?1
                  AND session_id = ?2
                  AND status = 'QUEUED'
                  AND sender_id IS NOT NULL
                  AND reply_to_message_id IS NOT NULL
                  AND (?3 IS NULL OR reply_to_message_id = ?3)
                ORDER BY created_at ASC
                LIMIT 1
            )
            UPDATE messages
            SET status = 'IN_PROGRESS',
                started_at = ?4
            WHERE message_id = (SELECT message_id FROM next)
            RETURNING
                message_id as "actor_message_id!: String",
                sender_id as "sender_actor_id: String",
                receiver_id as "actor_id!: String",
                session_id as "session_id: String",
                payload_json as "payload_json!: String",
                status as "status!: String",
                reply_to_sender_id as "reply_to_actor_id: String",
                reply_to_message_id as "reply_to_message_id: String",
                error as "error: String",
                created_at as "created_at!: String",
                started_at as "started_at: String",
                finished_at as "finished_at: String"
            "#,
            actor_id,
            session_id,
            expected_reply_to_message_id,
            now,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to claim next actor reply message")?;

        row.map(|row| {
            actor_mailbox_from_raw(
                row.actor_message_id,
                row.sender_actor_id,
                row.actor_id,
                row.session_id,
                row.payload_json,
                row.status,
                row.reply_to_actor_id,
                row.reply_to_message_id,
                row.error,
                row.created_at,
                row.started_at,
                row.finished_at,
            )
        })
        .transpose()
    }

    pub async fn ack_actor_message(&self, actor_message_id: &Uri) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let actor_message_id = actor_message_id.to_string();
        let updated = sqlx::query!(
            r#"
            UPDATE messages
            SET status = 'ACKED',
                finished_at = ?2,
                error = NULL
            WHERE message_id = ?1
            "#,
            actor_message_id,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to ack actor mailbox message")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn fail_actor_message(&self, actor_message_id: &Uri, error: &str) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let actor_message_id = actor_message_id.to_string();
        let updated = sqlx::query!(
            r#"
            UPDATE messages
            SET status = 'FAILED',
                finished_at = ?2,
                error = ?3
            WHERE message_id = ?1
            "#,
            actor_message_id,
            now,
            error,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to fail actor mailbox message")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn fail_stale_in_progress_messages(&self, older_than_seconds: u64) -> Result<u64> {
        let cutoff = Utc::now()
            - chrono::Duration::seconds(i64::try_from(older_than_seconds).unwrap_or(300));
        let cutoff_rfc3339 = cutoff.to_rfc3339();
        let finished_at = Utc::now().to_rfc3339();
        let error = "failed due to runtime restart while in progress";
        let updated = sqlx::query!(
            r#"
            UPDATE messages
            SET status = 'FAILED',
                finished_at = ?1,
                error = ?2
            WHERE status = 'IN_PROGRESS'
              AND started_at IS NOT NULL
              AND started_at <= ?3
            "#,
            finished_at,
            error,
            cutoff_rfc3339,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to fail stale in-progress actor mailbox messages")?
        .rows_affected();
        Ok(updated)
    }
}

fn actor_mailbox_from_raw(
    actor_message_id_raw: String,
    sender_actor_id_raw: Option<String>,
    actor_id_raw: String,
    session_id_raw: Option<String>,
    payload_raw: String,
    status: String,
    reply_to_actor_id_raw: Option<String>,
    reply_to_message_id_raw: Option<String>,
    error: Option<String>,
    created_at_raw: String,
    started_at_raw: Option<String>,
    finished_at_raw: Option<String>,
) -> Result<ActorMailboxRecord> {
    let payload: Value =
        serde_json::from_str(&payload_raw).context("invalid messages payload_json")?;
    let sender_actor_id = sender_actor_id_raw.as_deref().map(Uri::parse).transpose()?;
    let session_id = session_id_raw.as_deref().map(Uri::parse).transpose()?;
    let reply_to_actor_id = reply_to_actor_id_raw
        .as_deref()
        .map(Uri::parse)
        .transpose()?;
    let reply_to_message_id = reply_to_message_id_raw
        .as_deref()
        .map(Uri::parse)
        .transpose()?;
    let started_at = started_at_raw.as_deref().map(parse_ts).transpose()?;
    let finished_at = finished_at_raw.as_deref().map(parse_ts).transpose()?;
    Ok(ActorMailboxRecord {
        actor_message_id: Uri::parse(&actor_message_id_raw)?,
        sender_actor_id,
        actor_id: Uri::parse(&actor_id_raw)?,
        session_id,
        payload,
        status,
        reply_to_actor_id,
        reply_to_message_id,
        error,
        created_at: parse_ts(&created_at_raw)?,
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
                None,
                &serde_json::json!({"kind":"cast","n":1}),
                None,
                None,
            )
            .await?;
        let _m2 = db
            .enqueue_actor_message(
                &actor_id,
                None,
                &serde_json::json!({"kind":"cast","n":2}),
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
                None,
                &serde_json::json!({"kind":"call","task":"x"}),
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

        let msg_id_raw = msg_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT status as "status!: String"
            FROM messages
            WHERE message_id = ?1
            LIMIT 1
            "#,
            msg_id_raw,
        )
        .fetch_one(db.pool())
        .await?;
        assert_eq!(row.status, "FAILED");

        Ok(())
    }

    #[tokio::test]
    async fn list_actor_sessions_returns_distinct_recent() -> Result<()> {
        let path = tmp_db_path("list_sessions");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor_id = Uri::from_parts("devmode", "actor", Some("a3"))?;
        db.upsert_actor(&actor_id, "A3", "prompt", "RUNNING")
            .await?;

        let session_a = Uri::from_parts("borg", "session", Some("one"))?;
        let session_b = Uri::from_parts("borg", "session", Some("two"))?;
        db.enqueue_actor_message(
            &actor_id,
            Some(&session_a),
            &serde_json::json!({"kind":"cast","a":1}),
            None,
            None,
        )
        .await?;
        db.enqueue_actor_message(
            &actor_id,
            Some(&session_b),
            &serde_json::json!({"kind":"cast","b":1}),
            None,
            None,
        )
        .await?;
        db.enqueue_actor_message(
            &actor_id,
            Some(&session_a),
            &serde_json::json!({"kind":"cast","a":2}),
            None,
            None,
        )
        .await?;

        let sessions = db.list_actor_sessions(&actor_id, 10).await?;
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&session_a));
        assert!(sessions.contains(&session_b));
        Ok(())
    }
}
