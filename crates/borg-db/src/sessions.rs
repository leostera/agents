use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::Value;

use borg_core::{Uri, uri};

use crate::utils::parse_ts;
use crate::{BorgDb, SessionMessageRecord, SessionRecord};

impl BorgDb {
    pub async fn upsert_session(
        &self,
        session_id: &Uri,
        user_key: &Uri,
        port: &str,
        root_task_id: &Uri,
        state: &Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO sessions(session_id, user_key, port, root_task_id, state_json, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(session_id) DO UPDATE SET
                  user_key = excluded.user_key,
                  port = excluded.port,
                  root_task_id = excluded.root_task_id,
                  state_json = excluded.state_json,
                  updated_at = excluded.updated_at
                "#,
                (
                    session_id.to_string(),
                    user_key.to_string(),
                    port.to_string(),
                    root_task_id.to_string(),
                    state.to_string(),
                    now,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        limit: usize,
        port: Option<&str>,
        user_key: Option<&Uri>,
    ) -> Result<Vec<SessionRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut out = Vec::new();
        let mut rows = match (port, user_key) {
            (Some(port), Some(user_key)) => {
                self.conn
                    .query(
                        "SELECT session_id, user_key, port, root_task_id, state_json, updated_at FROM sessions WHERE port = ?1 AND user_key = ?2 ORDER BY updated_at DESC LIMIT ?3",
                        (port.to_string(), user_key.to_string(), limit),
                    )
                    .await?
            }
            (Some(port), None) => {
                self.conn
                    .query(
                        "SELECT session_id, user_key, port, root_task_id, state_json, updated_at FROM sessions WHERE port = ?1 ORDER BY updated_at DESC LIMIT ?2",
                        (port.to_string(), limit),
                    )
                    .await?
            }
            (None, Some(user_key)) => {
                self.conn
                    .query(
                        "SELECT session_id, user_key, port, root_task_id, state_json, updated_at FROM sessions WHERE user_key = ?1 ORDER BY updated_at DESC LIMIT ?2",
                        (user_key.to_string(), limit),
                    )
                    .await?
            }
            (None, None) => {
                self.conn
                    .query(
                        "SELECT session_id, user_key, port, root_task_id, state_json, updated_at FROM sessions ORDER BY updated_at DESC LIMIT ?1",
                        (limit,),
                    )
                    .await?
            }
        };

        while let Some(row) = rows.next().await? {
            let updated_at: String = row.get(5)?;
            out.push(SessionRecord {
                session_id: Uri::parse(&row.get::<String>(0)?)?,
                user_key: Uri::parse(&row.get::<String>(1)?)?,
                port: row.get(2)?,
                root_task_id: Uri::parse(&row.get::<String>(3)?)?,
                state: serde_json::from_str(&row.get::<String>(4)?).unwrap_or(Value::Null),
                updated_at: parse_ts(&updated_at)?,
            });
        }
        Ok(out)
    }

    pub async fn get_session(&self, session_id: &Uri) -> Result<Option<SessionRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT session_id, user_key, port, root_task_id, state_json, updated_at FROM sessions WHERE session_id = ?1 LIMIT 1",
                (session_id.to_string(),),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let updated_at: String = row.get(5)?;
        Ok(Some(SessionRecord {
            session_id: Uri::parse(&row.get::<String>(0)?)?,
            user_key: Uri::parse(&row.get::<String>(1)?)?,
            port: row.get(2)?,
            root_task_id: Uri::parse(&row.get::<String>(3)?)?,
            state: serde_json::from_str(&row.get::<String>(4)?).unwrap_or(Value::Null),
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
            out.push(serde_json::from_str(&raw).unwrap_or(Value::Null));
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
        Ok(Some(SessionMessageRecord {
            message_id: Uri::parse(&row.get::<String>(0)?)?,
            session_id: Uri::parse(&row.get::<String>(1)?)?,
            message_index: row.get(2)?,
            payload: serde_json::from_str(&row.get::<String>(3)?).unwrap_or(Value::Null),
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
