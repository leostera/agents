use anyhow::{Context, Result};
use borg_core::{Uri, uri};
use chrono::Utc;
use serde_json::Value;

use crate::{BorgDb, PortRecord};

impl BorgDb {
    pub async fn list_ports(&self, limit: usize) -> Result<Vec<PortRecord>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let mut rows = self
            .conn
            .query(
                r#"
                SELECT
                    p.port_id,
                    p.provider,
                    p.port_name,
                    p.enabled,
                    p.allows_guests,
                    p.default_agent_id,
                    p.settings_json,
                    COALESCE(sess.active_sessions, 0) AS active_sessions,
                    MAX(
                        COALESCE(p.updated_at, ''),
                        COALESCE(sess.updated_at, ''),
                        COALESCE(ctx.updated_at, '')
                    ) AS updated_at
                FROM ports p
                LEFT JOIN (
                    SELECT
                        port,
                        COUNT(*) AS active_sessions,
                        MAX(updated_at) AS updated_at
                    FROM sessions
                    GROUP BY port
                ) sess ON sess.port = ('borg:port:' || p.port_name)
                LEFT JOIN (
                    SELECT
                        port,
                        MAX(updated_at) AS updated_at
                    FROM port_session_ctx
                    GROUP BY port
                ) ctx ON ctx.port = p.port_name
                WHERE p.port_name != 'runtime'
                ORDER BY updated_at DESC, p.port_name ASC
                LIMIT ?1
                "#,
                (limit,),
            )
            .await
            .context("failed to list ports")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let port_id_raw: String = row.get(0)?;
            let provider: String = row.get(1)?;
            let port_name: String = row.get(2)?;
            let enabled_raw: i64 = row.get(3)?;
            let allows_guests_raw: i64 = row.get(4)?;
            let default_agent_id_raw: Option<String> = row.get(5)?;
            let settings_raw: String = row.get(6)?;
            let active_sessions: i64 = row.get(7)?;
            let updated_at_raw: String = row.get(8)?;
            let updated_at = if updated_at_raw.trim().is_empty() {
                None
            } else {
                Some(
                    chrono::DateTime::parse_from_rfc3339(&updated_at_raw)
                        .context("invalid RFC3339 timestamp in ports.updated_at")?
                        .with_timezone(&Utc),
                )
            };
            let default_agent_id = default_agent_id_raw
                .map(|raw| Uri::parse(&raw))
                .transpose()
                .context("invalid default_agent_id uri in ports table")?;
            let settings = serde_json::from_str(&settings_raw)
                .context("invalid settings_json in ports table")?;

            out.push(PortRecord {
                port_id: Uri::parse(&port_id_raw).context("invalid port_id uri in ports table")?,
                provider,
                port_name,
                enabled: enabled_raw != 0,
                allows_guests: allows_guests_raw != 0,
                default_agent_id,
                settings,
                active_sessions: active_sessions.max(0) as u64,
                updated_at,
            });
        }
        Ok(out)
    }

    pub async fn get_port(&self, port_name: &str) -> Result<Option<PortRecord>> {
        let mut rows = self
            .conn
            .query(
                r#"
                SELECT port_id, provider, port_name, enabled, allows_guests, default_agent_id, settings_json, updated_at
                FROM ports
                WHERE port_name = ?1
                LIMIT 1
                "#,
                (port_name.to_string(),),
            )
            .await
            .context("failed to get port")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let port_id_raw: String = row.get(0)?;
        let provider: String = row.get(1)?;
        let port_name: String = row.get(2)?;
        let enabled_raw: i64 = row.get(3)?;
        let allows_guests_raw: i64 = row.get(4)?;
        let default_agent_id_raw: Option<String> = row.get(5)?;
        let settings_raw: String = row.get(6)?;
        let updated_at_raw: String = row.get(7)?;
        let updated_at = if updated_at_raw.trim().is_empty() {
            None
        } else {
            Some(
                chrono::DateTime::parse_from_rfc3339(&updated_at_raw)
                    .context("invalid RFC3339 timestamp in ports.updated_at")?
                    .with_timezone(&Utc),
            )
        };

        let settings =
            serde_json::from_str(&settings_raw).context("invalid settings_json in ports table")?;
        let default_agent_id = default_agent_id_raw
            .map(|raw| Uri::parse(&raw))
            .transpose()
            .context("invalid default_agent_id uri in ports table")?;

        Ok(Some(PortRecord {
            port_id: Uri::parse(&port_id_raw).context("invalid port_id uri in ports table")?,
            provider,
            port_name,
            enabled: enabled_raw != 0,
            allows_guests: allows_guests_raw != 0,
            default_agent_id,
            settings,
            active_sessions: 0,
            updated_at,
        }))
    }

    pub async fn get_port_by_id(&self, port_id: &Uri) -> Result<Option<PortRecord>> {
        let mut rows = self
            .conn
            .query(
                r#"
                SELECT port_id, provider, port_name, enabled, allows_guests, default_agent_id, settings_json, updated_at
                FROM ports
                WHERE port_id = ?1
                LIMIT 1
                "#,
                (port_id.to_string(),),
            )
            .await
            .context("failed to get port by id")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let port_id_raw: String = row.get(0)?;
        let provider: String = row.get(1)?;
        let port_name: String = row.get(2)?;
        let enabled_raw: i64 = row.get(3)?;
        let allows_guests_raw: i64 = row.get(4)?;
        let default_agent_id_raw: Option<String> = row.get(5)?;
        let settings_raw: String = row.get(6)?;
        let updated_at_raw: String = row.get(7)?;
        let updated_at = if updated_at_raw.trim().is_empty() {
            None
        } else {
            Some(
                chrono::DateTime::parse_from_rfc3339(&updated_at_raw)
                    .context("invalid RFC3339 timestamp in ports.updated_at")?
                    .with_timezone(&Utc),
            )
        };

        let settings =
            serde_json::from_str(&settings_raw).context("invalid settings_json in ports table")?;
        let default_agent_id = default_agent_id_raw
            .map(|raw| Uri::parse(&raw))
            .transpose()
            .context("invalid default_agent_id uri in ports table")?;

        Ok(Some(PortRecord {
            port_id: Uri::parse(&port_id_raw).context("invalid port_id uri in ports table")?,
            provider,
            port_name,
            enabled: enabled_raw != 0,
            allows_guests: allows_guests_raw != 0,
            default_agent_id,
            settings,
            active_sessions: 0,
            updated_at,
        }))
    }

    pub async fn upsert_port(
        &self,
        port_name: &str,
        provider: &str,
        enabled: bool,
        allows_guests: bool,
        default_agent_id: Option<&Uri>,
        settings: &Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let existing = self.get_port(port_name).await?;
        let port_id = existing
            .as_ref()
            .map(|port| port.port_id.clone())
            .unwrap_or_else(|| uri!("borg", "port"));
        let settings_json = settings.to_string();

        self.conn
            .execute(
                r#"
                INSERT INTO ports(port_id, port_name, provider, enabled, allows_guests, default_agent_id, settings_json, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(port_name) DO UPDATE SET
                    provider = excluded.provider,
                    enabled = excluded.enabled,
                    allows_guests = excluded.allows_guests,
                    default_agent_id = excluded.default_agent_id,
                    settings_json = excluded.settings_json,
                    updated_at = excluded.updated_at
                "#,
                (
                    port_id.to_string(),
                    port_name.to_string(),
                    provider.to_string(),
                    if enabled { 1_i64 } else { 0_i64 },
                    if allows_guests { 1_i64 } else { 0_i64 },
                    default_agent_id.map(|uri| uri.to_string()),
                    settings_json,
                    now,
                ),
            )
            .await
            .context("failed to upsert port")?;

        Ok(())
    }

    pub async fn delete_port(&self, port_name: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM ports WHERE port_name = ?1",
                (port_name.to_string(),),
            )
            .await
            .context("failed deleting port record")?;
        self.conn
            .execute(
                "DELETE FROM port_session_ctx WHERE port = ?1",
                (port_name.to_string(),),
            )
            .await
            .context("failed deleting port session context")?;
        self.conn
            .execute(
                "DELETE FROM port_bindings WHERE port = ?1",
                (port_name.to_string(),),
            )
            .await
            .context("failed deleting port bindings")?;
        Ok(())
    }

    pub async fn upsert_port_setting(&self, port_name: &str, key: &str, value: &str) -> Result<()> {
        let mut port = self.get_port(port_name).await?.unwrap_or(PortRecord {
            port_id: uri!("borg", "port"),
            provider: if port_name == "telegram" {
                "telegram".to_string()
            } else {
                "custom".to_string()
            },
            port_name: port_name.to_string(),
            enabled: true,
            allows_guests: true,
            default_agent_id: None,
            settings: serde_json::json!({}),
            active_sessions: 0,
            updated_at: None,
        });

        match key {
            "kind" | "provider" => {
                port.provider = value.trim().to_string();
            }
            "enabled" => {
                port.enabled = parse_port_enabled(value);
            }
            "allows_guests" => {
                port.allows_guests = parse_port_enabled(value);
            }
            "default_agent_id" => {
                let trimmed = value.trim();
                port.default_agent_id = if trimmed.is_empty() {
                    None
                } else {
                    Some(Uri::parse(trimmed).context("invalid default_agent_id uri")?)
                };
            }
            _ => {
                if let Some(map) = port.settings.as_object_mut() {
                    map.insert(key.to_string(), Value::String(value.to_string()));
                } else {
                    port.settings = serde_json::json!({ key: value });
                }
            }
        }

        self.upsert_port(
            &port.port_name,
            &port.provider,
            port.enabled,
            port.allows_guests,
            port.default_agent_id.as_ref(),
            &port.settings,
        )
        .await
    }

    pub async fn get_port_setting(&self, port_name: &str, key: &str) -> Result<Option<String>> {
        let Some(port) = self.get_port(port_name).await? else {
            return Ok(None);
        };

        let value = match key {
            "kind" | "provider" => Some(port.provider),
            "enabled" => Some(if port.enabled { "true" } else { "false" }.to_string()),
            "allows_guests" => Some(if port.allows_guests { "true" } else { "false" }.to_string()),
            "default_agent_id" => port.default_agent_id.map(|uri| uri.to_string()),
            _ => port
                .settings
                .as_object()
                .and_then(|map| map.get(key))
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        Ok(value)
    }

    pub async fn list_port_settings(
        &self,
        port_name: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        let Some(port) = self.get_port(port_name).await? else {
            return Ok(out);
        };

        out.push(("provider".to_string(), port.provider));
        out.push((
            "enabled".to_string(),
            if port.enabled { "true" } else { "false" }.to_string(),
        ));
        out.push((
            "allows_guests".to_string(),
            if port.allows_guests { "true" } else { "false" }.to_string(),
        ));
        if let Some(default_agent_id) = port.default_agent_id {
            out.push(("default_agent_id".to_string(), default_agent_id.to_string()));
        }
        if let Some(map) = port.settings.as_object() {
            for (key, value) in map {
                if let Some(text) = value.as_str() {
                    out.push((key.to_string(), text.to_string()));
                } else {
                    out.push((key.to_string(), value.to_string()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.truncate(limit);
        Ok(out)
    }

    pub async fn delete_port_setting(&self, port_name: &str, key: &str) -> Result<u64> {
        let Some(mut port) = self.get_port(port_name).await? else {
            return Ok(0);
        };

        let changed = match key {
            "provider" | "kind" => {
                port.provider = "custom".to_string();
                true
            }
            "enabled" => {
                port.enabled = true;
                true
            }
            "allows_guests" => {
                port.allows_guests = true;
                true
            }
            "default_agent_id" => {
                port.default_agent_id = None;
                true
            }
            _ => port
                .settings
                .as_object_mut()
                .is_some_and(|map| map.remove(key).is_some()),
        };

        if !changed {
            return Ok(0);
        }

        self.upsert_port(
            &port.port_name,
            &port.provider,
            port.enabled,
            port.allows_guests,
            port.default_agent_id.as_ref(),
            &port.settings,
        )
        .await?;
        Ok(1)
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

    pub async fn get_port_binding_record(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<(Uri, Uri, Option<Uri>)>> {
        let mut rows = self
            .conn
            .query(
                "SELECT conversation_key, session_id, agent_id FROM port_bindings WHERE port = ?1 AND conversation_key = ?2 LIMIT 1",
                (port.to_string(), conversation_key.to_string()),
            )
            .await
            .context("failed to query port binding record")?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let conversation_key: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let agent_id: Option<String> = row.get(2)?;
        Ok(Some((
            Uri::parse(&conversation_key)?,
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

    pub async fn upsert_port_binding_record(
        &self,
        port: &str,
        conversation_key: &Uri,
        session_id: &Uri,
        agent_id: Option<&Uri>,
    ) -> Result<()> {
        self.upsert_port_binding(port, conversation_key, session_id, agent_id)
            .await
    }

    pub async fn list_port_bindings(
        &self,
        port: &str,
        limit: usize,
    ) -> Result<Vec<(Uri, Uri, Option<Uri>)>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let mut rows = self
            .conn
            .query(
                "SELECT conversation_key, session_id, agent_id FROM port_bindings WHERE port = ?1 ORDER BY updated_at DESC LIMIT ?2",
                (port.to_string(), limit),
            )
            .await
            .context("failed to list port bindings")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let conversation_key: String = row.get(0)?;
            let session_id: String = row.get(1)?;
            let agent_id: Option<String> = row.get(2)?;
            out.push((
                Uri::parse(&conversation_key)?,
                Uri::parse(&session_id)?,
                agent_id.map(|value| Uri::parse(&value)).transpose()?,
            ));
        }
        Ok(out)
    }

    pub async fn delete_port_binding(&self, port: &str, conversation_key: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM port_bindings WHERE port = ?1 AND conversation_key = ?2",
                (port.to_string(), conversation_key.to_string()),
            )
            .await
            .context("failed to delete port binding")?;
        Ok(deleted)
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

fn parse_port_enabled(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
}
