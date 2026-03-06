use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::Value;
use tracing::debug;

use borg_core::{Uri, uri};

use crate::utils::parse_ts;
use crate::{BorgDb, SessionMessageRecord, SessionRecord};

const RUNTIME_PORT_NAME: &str = "runtime";
const RUNTIME_PORT_URI: &str = "borg:port:runtime";

fn normalized_port_name(port: &Uri) -> String {
    port.as_str()
        .strip_prefix("borg:port:")
        .unwrap_or(port.as_str())
        .to_string()
}

fn normalized_port_filter(port: Option<&Uri>) -> Option<String> {
    port.map(normalized_port_name)
}

impl BorgDb {
    pub async fn upsert_session(&self, session_id: &Uri, port: &Uri) -> Result<()> {
        let port_name = normalized_port_name(port);
        self.upsert_port_binding_full_record(&port_name, session_id, session_id, None)
            .await
    }

    pub async fn list_sessions(
        &self,
        limit: usize,
        port: Option<&Uri>,
        _user_key: Option<&Uri>,
    ) -> Result<Vec<SessionRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let port_filter = normalized_port_filter(port);
        let include_runtime = if port_filter
            .as_deref()
            .is_none_or(|value| value == RUNTIME_PORT_NAME)
        {
            1_i64
        } else {
            0_i64
        };

        debug!(
            target: "borg_db",
            limit,
            port_filter = ?port_filter,
            "querying sessions from bindings/messages"
        );

        let rows = sqlx::query!(
            r#"
            WITH binding_rows AS (
                SELECT
                    pb.session_id as session_id,
                    ('borg:port:' || pb.port) as port,
                    MAX(pb.updated_at) as updated_at
                FROM port_bindings pb
                WHERE (?1 IS NULL OR pb.port = ?1)
                GROUP BY pb.session_id, pb.port
            ),
            runtime_rows AS (
                SELECT
                    m.session_id as session_id,
                    ?2 as port,
                    MAX(m.created_at) as updated_at
                FROM messages m
                WHERE ?3 = 1
                  AND m.session_id IS NOT NULL
                  AND m.receiver_id = m.session_id
                GROUP BY m.session_id
            ),
            candidates AS (
                SELECT session_id, port, updated_at FROM binding_rows
                UNION ALL
                SELECT session_id, port, updated_at FROM runtime_rows
            ),
            ranked AS (
                SELECT
                    session_id,
                    port,
                    updated_at,
                    ROW_NUMBER() OVER (
                        PARTITION BY session_id
                        ORDER BY updated_at DESC
                    ) as row_num
                FROM candidates
            )
            SELECT
                session_id as "session_id!: String",
                port as "port!: String",
                updated_at as "updated_at!: String"
            FROM ranked
            WHERE row_num = 1
            ORDER BY updated_at DESC
            LIMIT ?4
            "#,
            port_filter,
            RUNTIME_PORT_URI,
            include_runtime,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        let out = rows
            .into_iter()
            .map(|row| {
                Ok(SessionRecord {
                    session_id: Uri::parse(&row.session_id)?,
                    port: Uri::parse(&row.port)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        debug!(
            target: "borg_db",
            count = out.len(),
            limit,
            port_filter = ?port_filter,
            "sessions query completed"
        );
        Ok(out)
    }

    pub async fn get_session(&self, session_id: &Uri) -> Result<Option<SessionRecord>> {
        let session_id = session_id.to_string();
        let row = sqlx::query!(
            r#"
            WITH binding_rows AS (
                SELECT
                    ('borg:port:' || pb.port) as port,
                    MAX(pb.updated_at) as updated_at
                FROM port_bindings pb
                WHERE pb.session_id = ?1
                GROUP BY pb.port
            ),
            runtime_row AS (
                SELECT
                    ?2 as port,
                    MAX(m.created_at) as updated_at
                FROM messages m
                WHERE m.session_id = ?1
                  AND m.receiver_id = ?1
            ),
            candidates AS (
                SELECT port, updated_at FROM binding_rows
                UNION ALL
                SELECT port, updated_at FROM runtime_row
            )
            SELECT
                ?1 as "session_id!: String",
                port as "port!: String",
                updated_at as "updated_at!: String"
            FROM candidates
            WHERE updated_at IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            session_id,
            RUNTIME_PORT_URI,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(SessionRecord {
            session_id: Uri::parse(&row.session_id)?,
            port: Uri::parse(&row.port)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn delete_session(&self, session_id: &Uri) -> Result<u64> {
        let session_id = session_id.to_string();

        let deleted_messages =
            sqlx::query!("DELETE FROM messages WHERE session_id = ?1", session_id)
                .execute(self.conn.pool())
                .await?
                .rows_affected();

        let deleted_bindings = sqlx::query!(
            "DELETE FROM port_bindings WHERE session_id = ?1",
            session_id
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();

        if deleted_messages == 0 && deleted_bindings == 0 {
            Ok(0)
        } else {
            Ok(1)
        }
    }

    pub async fn ensure_session_row(&self, session_id: &Uri, port: &Uri) -> Result<()> {
        self.upsert_session(session_id, port).await
    }
}

impl BorgDb {
    pub async fn append_session_message(
        &self,
        session_id: &Uri,
        payload: &Value,
        reasoning_effort: Option<&str>,
    ) -> Result<Uri> {
        let session_id_raw = session_id.to_string();
        let now = Utc::now().to_rfc3339();
        let message_id = uri!("borg", "session_message");
        let message_id_raw = message_id.to_string();
        let message_id_ref = message_id_raw.as_str();
        let payload_json = payload.to_string();
        let payload_json_ref = payload_json.as_str();
        let session_id_ref = session_id_raw.as_str();
        let now_ref = now.as_str();

        sqlx::query!(
            "INSERT INTO messages(message_id, sender_id, receiver_id, session_id, payload_json, status, reply_to_sender_id, reply_to_message_id, error, created_at, started_at, finished_at) VALUES(?1, NULL, ?2, ?3, ?4, 'ACKED', NULL, NULL, NULL, ?5, ?5, ?5)",
            message_id_ref,
            session_id_ref,
            session_id_ref,
            payload_json_ref,
            now_ref,
        )
        .execute(self.conn.pool())
        .await?;

        if reasoning_effort.is_some() {
            self.set_session_reasoning_effort(session_id, reasoning_effort)
                .await?;
        }

        Ok(message_id)
    }

    pub async fn list_session_message_records(
        &self,
        session_id: &Uri,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SessionMessageRecord>> {
        let offset = i64::try_from(offset).unwrap_or(0);
        let limit = i64::try_from(limit).unwrap_or(100);
        let session_id = session_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT
                message_id as "message_id!: String",
                session_id as "session_id!: String",
                payload_json as "payload_json!: String",
                created_at as "created_at!: String"
            FROM messages
            WHERE session_id = ?1
              AND receiver_id = ?1
            ORDER BY created_at ASC, message_id ASC
            LIMIT ?2 OFFSET ?3"#,
            session_id,
            limit,
            offset,
        )
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                let payload = serde_json::from_str(&row.payload_json)
                    .map_err(|err| anyhow!("invalid session message payload_json: {err}"))?;
                Ok(SessionMessageRecord {
                    message_id: Uri::parse(&row.message_id)?,
                    session_id: Uri::parse(&row.session_id)?,
                    payload,
                    created_at: parse_ts(&row.created_at)?,
                })
            })
            .collect()
    }

    pub async fn list_session_messages(
        &self,
        session_id: &Uri,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Value>> {
        let records = self
            .list_session_message_records(session_id, from, limit)
            .await?;
        Ok(records.into_iter().map(|record| record.payload).collect())
    }

    pub async fn get_session_message_by_id(
        &self,
        session_id: &Uri,
        message_id: &Uri,
    ) -> Result<Option<SessionMessageRecord>> {
        let session_id = session_id.to_string();
        let message_id = message_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                message_id as "message_id!: String",
                session_id as "session_id!: String",
                payload_json as "payload_json!: String",
                created_at as "created_at!: String"
            FROM messages
            WHERE session_id = ?1
              AND receiver_id = ?1
              AND message_id = ?2
            LIMIT 1"#,
            session_id,
            message_id,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let payload = serde_json::from_str(&row.payload_json)
            .map_err(|err| anyhow!("invalid session message payload_json: {err}"))?;
        Ok(Some(SessionMessageRecord {
            message_id: Uri::parse(&row.message_id)?,
            session_id: Uri::parse(&row.session_id)?,
            payload,
            created_at: parse_ts(&row.created_at)?,
        }))
    }

    pub async fn get_session_message_offset_by_id(
        &self,
        session_id: &Uri,
        message_id: &Uri,
    ) -> Result<Option<usize>> {
        let session_id = session_id.to_string();
        let message_id = message_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT pos as "pos!: i64"
            FROM (
                SELECT
                    message_id,
                    (ROW_NUMBER() OVER (ORDER BY created_at ASC, message_id ASC) - 1) as pos
                FROM messages
                WHERE session_id = ?1
                  AND receiver_id = ?1
            ) ranked
            WHERE message_id = ?2
            LIMIT 1
            "#,
            session_id,
            message_id,
        )
        .fetch_optional(self.conn.pool())
        .await?;
        Ok(row.map(|entry| usize::try_from(entry.pos).unwrap_or(0)))
    }

    pub async fn update_session_message_by_id(
        &self,
        session_id: &Uri,
        message_id: &Uri,
        payload: &Value,
    ) -> Result<u64> {
        let payload_json = payload.to_string();
        let session_id = session_id.to_string();
        let message_id = message_id.to_string();
        let updated = sqlx::query!(
            "UPDATE messages SET payload_json = ?1 WHERE session_id = ?2 AND receiver_id = ?2 AND message_id = ?3",
            payload_json,
            session_id,
            message_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(updated)
    }

    pub async fn delete_session_message_by_id(
        &self,
        session_id: &Uri,
        message_id: &Uri,
    ) -> Result<u64> {
        let session_id = session_id.to_string();
        let message_id = message_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM messages WHERE session_id = ?1 AND receiver_id = ?1 AND message_id = ?2",
            session_id,
            message_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(deleted)
    }

    pub async fn count_session_messages(&self, session_id: &Uri) -> Result<usize> {
        let session_id = session_id.to_string();
        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64"
            FROM messages
            WHERE session_id = ?1
              AND receiver_id = ?1"#,
            session_id,
        )
        .fetch_one(self.conn.pool())
        .await?;
        Ok(usize::try_from(row.count).unwrap_or(0))
    }

    pub async fn clear_session_history(&self, session_id: &Uri) -> Result<u64> {
        let session_id = session_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM messages WHERE session_id = ?1 AND receiver_id = ?1",
            session_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(deleted)
    }

    pub async fn get_session_reasoning_effort(&self, session_id: &Uri) -> Result<Option<String>> {
        let session_id = session_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT current_reasoning_effort as "current_reasoning_effort?: String"
            FROM port_bindings
            WHERE session_id = ?1
              AND current_reasoning_effort IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            session_id
        )
        .fetch_optional(self.conn.pool())
        .await?;
        let value = row.and_then(|entry| entry.current_reasoning_effort);
        Ok(value
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty()))
    }

    pub async fn set_session_reasoning_effort(
        &self,
        session_id: &Uri,
        reasoning_effort: Option<&str>,
    ) -> Result<()> {
        let session_id_raw = session_id.to_string();
        let session_id_ref = session_id_raw.as_str();
        let now = Utc::now().to_rfc3339();
        let now_ref = now.as_str();
        let normalized = reasoning_effort
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let normalized_ref = normalized.as_deref();

        let updated = sqlx::query!(
            r#"
            UPDATE port_bindings
            SET current_reasoning_effort = ?1,
                updated_at = ?2
            WHERE session_id = ?3
            "#,
            normalized_ref,
            now_ref,
            session_id_ref
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();

        if updated == 0 {
            self.upsert_port_binding_full_record(RUNTIME_PORT_NAME, session_id, session_id, None)
                .await?;

            sqlx::query!(
                r#"
                UPDATE port_bindings
                SET current_reasoning_effort = ?1,
                    updated_at = ?2
                WHERE port = ?3
                  AND conversation_key = ?4
                "#,
                normalized_ref,
                now_ref,
                RUNTIME_PORT_NAME,
                session_id_ref
            )
            .execute(self.conn.pool())
            .await?;
        }

        Ok(())
    }
}
