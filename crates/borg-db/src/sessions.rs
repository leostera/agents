use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::Value;
use tracing::debug;

use borg_core::{Uri, uri};

use crate::utils::parse_ts;
use crate::{BorgDb, SessionMessageRecord, SessionRecord};

impl BorgDb {
    pub async fn upsert_session(&self, session_id: &Uri, users: &[Uri], port: &Uri) -> Result<()> {
        if users.is_empty() {
            return Err(anyhow!("session requires at least one user"));
        }
        let users_json =
            serde_json::to_string(&users.iter().map(ToString::to_string).collect::<Vec<_>>())?;
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO sessions(session_id, users_json, port, updated_at)
                VALUES(?1, ?2, ?3, ?4)
                ON CONFLICT(session_id) DO UPDATE SET
                  users_json = excluded.users_json,
                  port = excluded.port,
                  updated_at = excluded.updated_at
                "#,
                (session_id.to_string(), users_json, port.to_string(), now),
            )
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
        let mut out = Vec::new();
        let mut rows = match (port, user_key) {
            (Some(port), Some(user_key)) => {
                self.conn
                    .query(
                        "SELECT session_id, users_json, port, updated_at FROM sessions WHERE port = ?1 AND users_json LIKE ?2 ORDER BY updated_at DESC LIMIT ?3",
                        (
                            port.to_string(),
                            format!("%{}%", user_key),
                            limit,
                        ),
                    )
                    .await?
            }
            (Some(port), None) => {
                self.conn
                    .query(
                        "SELECT session_id, users_json, port, updated_at FROM sessions WHERE port = ?1 ORDER BY updated_at DESC LIMIT ?2",
                        (port.to_string(), limit),
                    )
                    .await?
            }
            (None, Some(user_key)) => {
                self.conn
                    .query(
                        "SELECT session_id, users_json, port, updated_at FROM sessions WHERE users_json LIKE ?1 ORDER BY updated_at DESC LIMIT ?2",
                        (format!("%{}%", user_key), limit),
                    )
                    .await?
            }
            (None, None) => {
                self.conn
                    .query(
                        "SELECT session_id, users_json, port, updated_at FROM sessions ORDER BY updated_at DESC LIMIT ?1",
                        (limit,),
                    )
                    .await?
            }
        };

        while let Some(row) = rows.next().await? {
            let updated_at: String = row.get(3)?;
            let users_json: String = row.get(1)?;
            let users = parse_users_json(&users_json, "list_sessions")?;
            out.push(SessionRecord {
                session_id: Uri::parse(&row.get::<String>(0)?)?,
                users,
                port: Uri::parse(&row.get::<String>(2)?)?,
                updated_at: parse_ts(&updated_at)?,
            });
        }
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
        let mut rows = self
            .conn
            .query(
                "SELECT session_id, users_json, port, updated_at FROM sessions WHERE session_id = ?1 LIMIT 1",
                (session_id.to_string(),),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let updated_at: String = row.get(3)?;
        let users_json: String = row.get(1)?;
        let users = parse_users_json(&users_json, "get_session")?;
        Ok(Some(SessionRecord {
            session_id: Uri::parse(&row.get::<String>(0)?)?,
            users,
            port: Uri::parse(&row.get::<String>(2)?)?,
            updated_at: parse_ts(&updated_at)?,
        }))
    }

    pub async fn delete_session(&self, session_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM sessions WHERE session_id = ?1",
                (session_id.to_string(),),
            )
            .await?;
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
        let mut rows = self
            .conn
            .query(
                "SELECT COALESCE(MAX(message_index), -1) + 1 FROM session_messages WHERE session_id = ?1",
                (session_id.to_string(),),
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow!("failed to allocate session message index"))?;
        let next_index: i64 = row.get(0)?;
        let now = Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO session_messages(message_id, session_id, message_index, payload_json, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
                (
                    uri!("borg", "session_message").to_string(),
                    session_id.to_string(),
                    next_index,
                    payload.to_string(),
                    now,
                ),
            )
            .await?;

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
        let mut rows = self
            .conn
            .query(
                "SELECT payload_json FROM session_messages WHERE session_id = ?1 AND message_index >= ?2 ORDER BY message_index ASC LIMIT ?3",
                (session_id.to_string(), from, limit),
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let raw: String = row.get(0)?;
            out.push(
                serde_json::from_str(&raw)
                    .map_err(|err| anyhow!("invalid session message payload_json: {err}"))?,
            );
        }
        Ok(out)
    }

    pub async fn get_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
    ) -> Result<Option<SessionMessageRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT message_id, session_id, message_index, payload_json, created_at FROM session_messages WHERE session_id = ?1 AND message_index = ?2 LIMIT 1",
                (session_id.to_string(), message_index),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let created_at: String = row.get(4)?;
        let payload_json: String = row.get(3)?;
        let payload = serde_json::from_str(&payload_json)
            .map_err(|err| anyhow!("invalid session message payload_json: {err}"))?;
        Ok(Some(SessionMessageRecord {
            message_id: Uri::parse(&row.get::<String>(0)?)?,
            session_id: Uri::parse(&row.get::<String>(1)?)?,
            message_index: row.get(2)?,
            payload,
            created_at: parse_ts(&created_at)?,
        }))
    }

    pub async fn update_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
        payload: &Value,
    ) -> Result<u64> {
        let updated = self
            .conn
            .execute(
                "UPDATE session_messages SET payload_json = ?1 WHERE session_id = ?2 AND message_index = ?3",
                (payload.to_string(), session_id.to_string(), message_index),
            )
            .await?;
        Ok(updated)
    }

    pub async fn delete_session_message(
        &self,
        session_id: &Uri,
        message_index: i64,
    ) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM session_messages WHERE session_id = ?1 AND message_index = ?2",
                (session_id.to_string(), message_index),
            )
            .await?;
        Ok(deleted)
    }

    pub async fn count_session_messages(&self, session_id: &Uri) -> Result<usize> {
        let mut rows = self
            .conn
            .query(
                "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1",
                (session_id.to_string(),),
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow!("failed counting session messages"))?;
        let count: i64 = row.get(0)?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    pub async fn clear_session_history(&self, session_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM session_messages WHERE session_id = ?1",
                (session_id.to_string(),),
            )
            .await?;
        Ok(deleted)
    }
}
