use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::{Value, json};
use tracing::debug;

use borg_core::{Uri, uri};

use crate::utils::parse_ts;
use crate::{BorgDb, SessionMessageRecord, SessionRecord};

impl BorgDb {
    pub async fn upsert_session(&self, session_id: &Uri, users: &[Uri], port: &Uri) -> Result<()> {
        if users.is_empty() {
            return Err(anyhow!("session requires at least one user"));
        }
        let session_id = session_id.to_string();
        let users_json =
            serde_json::to_string(&users.iter().map(ToString::to_string).collect::<Vec<_>>())?;
        let port = port.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO sessions(session_id, users_json, port, updated_at)
            VALUES(?1, ?2, ?3, ?4)
            ON CONFLICT(session_id) DO UPDATE SET
              users_json = excluded.users_json,
              port = excluded.port,
              updated_at = excluded.updated_at
            "#,
            session_id,
            users_json,
            port,
            now,
        )
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        limit: usize,
        port: Option<&Uri>,
        user_key: Option<&Uri>,
    ) -> Result<Vec<SessionRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        debug!(
            target: "borg_db",
            limit,
            port = ?port,
            user_key = ?user_key.map(ToString::to_string),
            "querying sessions"
        );
        let port_filter = port.map(ToString::to_string);
        let user_like_filter = user_key.map(|value| format!("%{}%", value));
        let rows = sqlx::query!(
            r#"SELECT
                session_id as "session_id!: String",
                users_json as "users_json!: String",
                port as "port!: String",
                updated_at as "updated_at!: String"
            FROM sessions
            WHERE (?1 IS NULL OR port = ?1)
              AND (?2 IS NULL OR users_json LIKE ?2)
            ORDER BY updated_at DESC
            LIMIT ?3"#,
            port_filter,
            user_like_filter,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        let out = rows
            .into_iter()
            .map(|row| {
                let users = parse_users_json(&row.users_json, "list_sessions")?;
                Ok(SessionRecord {
                    session_id: Uri::parse(&row.session_id)?,
                    users,
                    port: Uri::parse(&row.port)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        debug!(
            target: "borg_db",
            count = out.len(),
            limit,
            port = ?port.map(ToString::to_string),
            user_key = ?user_key.map(ToString::to_string),
            "sessions query completed"
        );
        Ok(out)
    }

    pub async fn get_session(&self, session_id: &Uri) -> Result<Option<SessionRecord>> {
        let session_id = session_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                session_id as "session_id!: String",
                users_json as "users_json!: String",
                port as "port!: String",
                updated_at as "updated_at!: String"
            FROM sessions
            WHERE session_id = ?1
            LIMIT 1"#,
            session_id,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let users = parse_users_json(&row.users_json, "get_session")?;
        Ok(Some(SessionRecord {
            session_id: Uri::parse(&row.session_id)?,
            users,
            port: Uri::parse(&row.port)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn delete_session(&self, session_id: &Uri) -> Result<u64> {
        let session_id = session_id.to_string();
        let deleted = sqlx::query!("DELETE FROM sessions WHERE session_id = ?1", session_id,)
            .execute(self.conn.pool())
            .await?
            .rows_affected();
        Ok(deleted)
    }
}

fn parse_users_json(users_json: &str, context: &str) -> Result<Vec<Uri>> {
    if users_json.trim().is_empty() {
        return Err(anyhow!("invalid empty session users_json for {context}"));
    }
    let raw_users: Vec<String> = serde_json::from_str(users_json)
        .map_err(|err| anyhow!("invalid session users_json for {context}: {err}"))?;
    if raw_users.is_empty() {
        return Err(anyhow!("session users_json has no users for {context}"));
    }
    raw_users
        .into_iter()
        .map(|value| Uri::parse(&value))
        .collect::<Result<Vec<_>>>()
}

impl BorgDb {
    pub async fn append_session_message(&self, session_id: &Uri, payload: &Value) -> Result<i64> {
        let session_id = session_id.to_string();
        let session_for_index = session_id.clone();
        let row = sqlx::query!(
            r#"SELECT
                COALESCE(MAX(message_index), -1) + 1 as "next_index!: i64"
            FROM session_messages
            WHERE session_id = ?1"#,
            session_for_index,
        )
        .fetch_one(self.conn.pool())
        .await?;
        let next_index = row.next_index;
        let now = Utc::now().to_rfc3339();
        let message_id = uri!("borg", "session_message").to_string();
        let payload_json = payload.to_string();

        sqlx::query!(
            "INSERT INTO session_messages(message_id, session_id, message_index, payload_json, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            message_id,
            session_id,
            next_index,
            payload_json,
            now,
        )
        .execute(self.conn.pool())
        .await?;

        let existing_ctx = sqlx::query!(
            r#"SELECT context_snapshot_json as "context_snapshot_json: String"
            FROM sessions
            WHERE session_id = ?1
            LIMIT 1"#,
            session_id
        )
        .fetch_optional(self.conn.pool())
        .await?
        .and_then(|row| row.context_snapshot_json);

        let mut snapshot = existing_ctx
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or_else(|| json!({}));

        if !snapshot.is_object() {
            snapshot = json!({});
        }

        if let Some(obj) = snapshot.as_object_mut() {
            obj.insert(
                "_session".to_string(),
                json!({
                    "last_message_index": next_index,
                    "updated_at": now,
                    "last_message": payload
                }),
            );
        }

        let snapshot_json = snapshot.to_string();
        let updated = sqlx::query!(
            r#"
            UPDATE sessions
            SET context_snapshot_json = ?1, updated_at = ?2
            WHERE session_id = ?3
            "#,
            snapshot_json,
            now,
            session_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();

        if updated == 0 {
            let users_json = json!(["borg:user:system"]).to_string();
            let port = "borg:port:runtime".to_string();
            let snapshot_json = snapshot.to_string();
            sqlx::query!(
                r#"
                INSERT INTO sessions(
                    session_id,
                    users_json,
                    port,
                    context_snapshot_json,
                    updated_at
                )
                VALUES(?1, ?2, ?3, ?4, ?5)
                "#,
                session_id,
                users_json,
                port,
                snapshot_json,
                now,
            )
            .execute(self.conn.pool())
            .await?;
        }

        Ok(next_index)
    }

    pub async fn list_session_messages(
        &self,
        session_id: &Uri,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Value>> {
        let from = i64::try_from(from).unwrap_or(0);
        let limit = i64::try_from(limit).unwrap_or(100);
        let session_id = session_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT payload_json as "payload_json!: String"
            FROM session_messages
            WHERE session_id = ?1 AND message_index >= ?2
            ORDER BY message_index ASC
            LIMIT ?3"#,
            session_id,
            from,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                serde_json::from_str(&row.payload_json)
                    .map_err(|err| anyhow!("invalid session message payload_json: {err}"))
            })
            .collect()
    }

    pub async fn get_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
    ) -> Result<Option<SessionMessageRecord>> {
        let session_id = session_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                message_id as "message_id!: String",
                session_id as "session_id!: String",
                message_index as "message_index!: i64",
                payload_json as "payload_json!: String",
                created_at as "created_at!: String"
            FROM session_messages
            WHERE session_id = ?1 AND message_index = ?2
            LIMIT 1"#,
            session_id,
            message_index,
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
            message_index: row.message_index,
            payload,
            created_at: parse_ts(&row.created_at)?,
        }))
    }

    pub async fn update_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
        payload: &Value,
    ) -> Result<u64> {
        let payload_json = payload.to_string();
        let session_id = session_id.to_string();
        let updated = sqlx::query!(
            "UPDATE session_messages SET payload_json = ?1 WHERE session_id = ?2 AND message_index = ?3",
            payload_json,
            session_id,
            message_index,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(updated)
    }

    pub async fn delete_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
    ) -> Result<u64> {
        let session_id = session_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM session_messages WHERE session_id = ?1 AND message_index = ?2",
            session_id,
            message_index,
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
            FROM session_messages
            WHERE session_id = ?1"#,
            session_id,
        )
        .fetch_one(self.conn.pool())
        .await?;
        let count = row.count;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    pub async fn clear_session_history(&self, session_id: &Uri) -> Result<u64> {
        let session_id = session_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM session_messages WHERE session_id = ?1",
            session_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(deleted)
    }
}
