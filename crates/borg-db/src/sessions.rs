use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::Value;

use borg_core::{Uri, uri};

use crate::BorgDb;

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
