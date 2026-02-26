use anyhow::{Context, Result};
use borg_core::{Uri, uri};
use chrono::Utc;
use serde_json::Value;

use crate::BorgDb;

impl BorgDb {
    pub async fn upsert_port_setting(&self, port: &str, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO port_settings(port, key, value, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(port, key) DO UPDATE SET
                  value = excluded.value,
                  updated_at = excluded.updated_at
                "#,
                (
                    port.to_string(),
                    key.to_string(),
                    value.to_string(),
                    now.clone(),
                    now,
                ),
            )
            .await
            .context("failed to upsert port setting")?;
        Ok(())
    }

    pub async fn get_port_setting(&self, port: &str, key: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT value FROM port_settings WHERE port = ?1 AND key = ?2 LIMIT 1",
                (port.to_string(), key.to_string()),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        Ok(Some(row.get(0)?))
    }

    pub async fn resolve_port_session(
        &self,
        port: &str,
        conversation_key: &Uri,
        requested_session_id: Option<&Uri>,
        requested_agent_id: Option<&Uri>,
    ) -> Result<(Uri, Option<Uri>)> {
        if let Some(session_id) = requested_session_id {
            self.upsert_port_binding(port, conversation_key, session_id, requested_agent_id)
                .await?;
            return Ok((session_id.clone(), requested_agent_id.cloned()));
        }

        if let Some(existing) = self.get_port_binding(port, conversation_key).await? {
            return Ok(existing);
        }

        let session_id = uri!("borg", "session");
        self.upsert_port_binding(port, conversation_key, &session_id, requested_agent_id)
            .await?;
        Ok((session_id, requested_agent_id.cloned()))
    }

    async fn get_port_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<(Uri, Option<Uri>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT session_id, agent_id FROM port_bindings WHERE port = ?1 AND conversation_key = ?2 LIMIT 1",
                (port.to_string(), conversation_key.to_string()),
            )
            .await
            .context("failed to query port binding")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let session_id: String = row.get(0)?;
        let agent_id: Option<String> = row.get(1)?;
        Ok(Some((
            Uri::parse(&session_id)?,
            agent_id.map(|value| Uri::parse(&value)).transpose()?,
        )))
    }

    async fn upsert_port_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
        session_id: &Uri,
        agent_id: Option<&Uri>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO port_bindings(port, conversation_key, session_id, agent_id, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(port, conversation_key) DO UPDATE SET
                  session_id = excluded.session_id,
                  agent_id = excluded.agent_id,
                  updated_at = excluded.updated_at
                "#,
                (
                    port.to_string(),
                    conversation_key.to_string(),
                    session_id.to_string(),
                    agent_id.map(|value| value.to_string()),
                    now.clone(),
                    now,
                ),
            )
            .await
            .context("failed to upsert port binding")?;
        Ok(())
    }

    pub async fn upsert_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
        ctx: &Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO port_session_ctx(port, session_id, ctx_json, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(port, session_id) DO UPDATE SET
                  ctx_json = excluded.ctx_json,
                  updated_at = excluded.updated_at
                "#,
                (
                    port.to_string(),
                    session_id.to_string(),
                    ctx.to_string(),
                    now.clone(),
                    now,
                ),
            )
            .await
            .context("failed to upsert port session context")?;
        Ok(())
    }

    pub async fn get_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
    ) -> Result<Option<Value>> {
        let mut rows = self
            .conn
            .query(
                "SELECT ctx_json FROM port_session_ctx WHERE port = ?1 AND session_id = ?2 LIMIT 1",
                (port.to_string(), session_id.to_string()),
            )
            .await
            .context("failed to query port session context")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let raw: String = row.get(0)?;
        let parsed = serde_json::from_str(&raw).context("invalid port session context json")?;
        Ok(Some(parsed))
    }

    pub async fn get_any_port_session_context(
        &self,
        session_id: &Uri,
    ) -> Result<Option<(String, Value)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT port, ctx_json FROM port_session_ctx WHERE session_id = ?1 ORDER BY updated_at DESC LIMIT 1",
                (session_id.to_string(),),
            )
            .await
            .context("failed to query session port context")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let port: String = row.get(0)?;
        let raw: String = row.get(1)?;
        let parsed = serde_json::from_str(&raw).context("invalid session port context json")?;
        Ok(Some((port, parsed)))
    }

    pub async fn list_port_session_ids(&self, port: &str) -> Result<Vec<Uri>> {
        let mut rows = self
            .conn
            .query(
                "SELECT DISTINCT session_id FROM port_bindings WHERE port = ?1",
                (port.to_string(),),
            )
            .await
            .context("failed to list port session ids")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let raw: String = row.get(0)?;
            out.push(Uri::parse(&raw)?);
        }
        Ok(out)
    }

    pub async fn clear_port_session_context(&self, port: &str, session_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM port_session_ctx WHERE port = ?1 AND session_id = ?2",
                (port.to_string(), session_id.to_string()),
            )
            .await
            .context("failed to clear port session context")?;
        Ok(deleted)
    }
}
