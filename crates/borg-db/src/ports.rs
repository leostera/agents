use anyhow::{Context, Result};
use borg_core::{Uri, uri};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;

use crate::{BorgDb, PortRecord};

fn default_provider_for_port_name(port_name: &str) -> String {
    match port_name.trim().to_ascii_lowercase().as_str() {
        "telegram" => "telegram".to_string(),
        "discord" => "discord".to_string(),
        _ => "custom".to_string(),
    }
}

fn is_actor_uri(uri: &Uri) -> bool {
    uri.as_str().contains(":actor:")
}

fn parse_assigned_actor_id(settings: &Value, default_actor_id: Option<Uri>) -> Result<Option<Uri>> {
    if let Some(raw) = settings
        .as_object()
        .and_then(|map| map.get("actor_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        return Uri::parse(raw)
            .map(Some)
            .context("invalid actor_id uri in ports.settings_json");
    }

    Ok(default_actor_id.filter(is_actor_uri))
}

enum ActorBindingUpdate<'a> {
    Preserve,
    Set(Option<&'a Uri>),
}

impl BorgDb {
    pub async fn list_ports(&self, limit: usize) -> Result<Vec<PortRecord>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let rows = sqlx::query!(
            r#"
            SELECT
                p.port_id as "port_id!: String",
                p.provider as "provider!: String",
                p.port_name as "port_name!: String",
                p.enabled as "enabled!: i64",
                p.allows_guests as "allows_guests!: i64",
                p.default_actor_id as "default_actor_id: String",
                p.settings_json as "settings_json!: String",
                COALESCE(bind.active_bindings, 0) as "active_bindings!: i64",
                MAX(
                    COALESCE(p.updated_at, ''),
                    COALESCE(bind.updated_at, '')
                ) as "updated_at_effective!: String"
            FROM ports p
            LEFT JOIN (
                SELECT
                    port,
                    COUNT(*) AS active_bindings,
                    MAX(updated_at) AS updated_at
                FROM port_bindings
                GROUP BY port
            ) bind ON bind.port = p.port_name
            WHERE p.port_name != 'runtime'
            ORDER BY 9 DESC, p.port_name ASC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list ports")?;

        rows.into_iter()
            .map(|row| {
                let updated_at = if row.updated_at_effective.trim().is_empty() {
                    None
                } else {
                    Some(
                        chrono::DateTime::parse_from_rfc3339(&row.updated_at_effective)
                            .context("invalid RFC3339 timestamp in ports.updated_at")?
                            .with_timezone(&Utc),
                    )
                };
                let default_actor_id = row
                    .default_actor_id
                    .as_deref()
                    .map(Uri::parse)
                    .transpose()
                    .context("invalid default_actor_id uri in ports table")?;
                let settings = serde_json::from_str(&row.settings_json)
                    .context("invalid settings_json in ports table")?;

                Ok(PortRecord {
                    port_id: Uri::parse(&row.port_id)
                        .context("invalid port_id uri in ports table")?,
                    provider: row.provider,
                    port_name: row.port_name,
                    enabled: row.enabled != 0,
                    allows_guests: row.allows_guests != 0,
                    assigned_actor_id: parse_assigned_actor_id(
                        &settings,
                        default_actor_id.clone(),
                    )?,
                    default_actor_id,
                    settings,
                    active_bindings: row.active_bindings.max(0) as u64,
                    updated_at,
                })
            })
            .collect()
    }

    pub async fn get_port(&self, port_name: &str) -> Result<Option<PortRecord>> {
        let port_name = port_name.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                port_id as "port_id!: String",
                provider as "provider!: String",
                port_name as "port_name!: String",
                enabled as "enabled!: i64",
                allows_guests as "allows_guests!: i64",
                default_actor_id as "default_actor_id: String",
                settings_json as "settings_json!: String",
                updated_at as "updated_at!: String"
            FROM ports
            WHERE port_name = ?1
            LIMIT 1
            "#,
            port_name,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get port")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let updated_at = if row.updated_at.trim().is_empty() {
            None
        } else {
            Some(
                chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .context("invalid RFC3339 timestamp in ports.updated_at")?
                    .with_timezone(&Utc),
            )
        };

        let settings = serde_json::from_str(&row.settings_json)
            .context("invalid settings_json in ports table")?;
        let default_actor_id = row
            .default_actor_id
            .map(|raw| Uri::parse(&raw))
            .transpose()
            .context("invalid default_actor_id uri in ports table")?;

        Ok(Some(PortRecord {
            port_id: Uri::parse(&row.port_id).context("invalid port_id uri in ports table")?,
            provider: row.provider,
            port_name: row.port_name,
            enabled: row.enabled != 0,
            allows_guests: row.allows_guests != 0,
            assigned_actor_id: parse_assigned_actor_id(&settings, default_actor_id.clone())?,
            default_actor_id,
            settings,
            active_bindings: 0,
            updated_at,
        }))
    }

    pub async fn get_port_by_id(&self, port_id: &Uri) -> Result<Option<PortRecord>> {
        let port_id = port_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                port_id as "port_id!: String",
                provider as "provider!: String",
                port_name as "port_name!: String",
                enabled as "enabled!: i64",
                allows_guests as "allows_guests!: i64",
                default_actor_id as "default_actor_id: String",
                settings_json as "settings_json!: String",
                updated_at as "updated_at!: String"
            FROM ports
            WHERE port_id = ?1
            LIMIT 1
            "#,
            port_id,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get port by id")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let updated_at = if row.updated_at.trim().is_empty() {
            None
        } else {
            Some(
                chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .context("invalid RFC3339 timestamp in ports.updated_at")?
                    .with_timezone(&Utc),
            )
        };

        let settings = serde_json::from_str(&row.settings_json)
            .context("invalid settings_json in ports table")?;
        let default_actor_id = row
            .default_actor_id
            .map(|raw| Uri::parse(&raw))
            .transpose()
            .context("invalid default_actor_id uri in ports table")?;

        Ok(Some(PortRecord {
            port_id: Uri::parse(&row.port_id).context("invalid port_id uri in ports table")?,
            provider: row.provider,
            port_name: row.port_name,
            enabled: row.enabled != 0,
            allows_guests: row.allows_guests != 0,
            assigned_actor_id: parse_assigned_actor_id(&settings, default_actor_id.clone())?,
            default_actor_id,
            settings,
            active_bindings: 0,
            updated_at,
        }))
    }

    pub async fn upsert_port(
        &self,
        port_name: &str,
        provider: &str,
        enabled: bool,
        allows_guests: bool,
        default_actor_id: Option<&Uri>,
        settings: &Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let existing = self.get_port(port_name).await?;
        let port_id = existing
            .as_ref()
            .map(|port| port.port_id.clone())
            .unwrap_or_else(|| uri!("borg", "port"));
        let settings_json = settings.to_string();
        let port_id = port_id.to_string();
        let port_name = port_name.to_string();
        let provider = provider.to_string();
        let default_actor_id = default_actor_id.map(|uri| uri.to_string());
        let enabled_raw = if enabled { 1_i64 } else { 0_i64 };
        let allows_guests_raw = if allows_guests { 1_i64 } else { 0_i64 };

        sqlx::query!(
            r#"
            INSERT INTO ports(port_id, port_name, provider, enabled, allows_guests, default_actor_id, settings_json, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(port_name) DO UPDATE SET
                provider = excluded.provider,
                enabled = excluded.enabled,
                allows_guests = excluded.allows_guests,
                default_actor_id = excluded.default_actor_id,
                settings_json = excluded.settings_json,
                updated_at = excluded.updated_at
            "#,
            port_id,
            port_name,
            provider,
            enabled_raw,
            allows_guests_raw,
            default_actor_id,
            settings_json,
            now,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to upsert port")?;

        Ok(())
    }

    pub async fn delete_port(&self, port_name: &str) -> Result<()> {
        let port_name = port_name.to_string();
        let port_for_ports = port_name.clone();
        sqlx::query!("DELETE FROM ports WHERE port_name = ?1", port_for_ports,)
            .execute(self.conn.pool())
            .await
            .context("failed deleting port record")?;
        sqlx::query!("DELETE FROM port_bindings WHERE port = ?1", port_name,)
            .execute(self.conn.pool())
            .await
            .context("failed deleting port bindings")?;
        Ok(())
    }

    pub async fn upsert_port_setting(&self, port_name: &str, key: &str, value: &str) -> Result<()> {
        let mut port = self.get_port(port_name).await?.unwrap_or(PortRecord {
            port_id: uri!("borg", "port"),
            provider: default_provider_for_port_name(port_name),
            port_name: port_name.to_string(),
            enabled: true,
            allows_guests: true,
            assigned_actor_id: None,
            default_actor_id: None,
            settings: serde_json::json!({}),
            active_bindings: 0,
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
            "default_actor_id" => {
                let trimmed = value.trim();
                port.default_actor_id = if trimmed.is_empty() {
                    None
                } else {
                    Some(Uri::parse(trimmed).context("invalid default_actor_id uri")?)
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
            port.default_actor_id.as_ref(),
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
            "default_actor_id" => port.default_actor_id.map(|uri| uri.to_string()),
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
        if let Some(default_actor_id) = port.default_actor_id {
            out.push(("default_actor_id".to_string(), default_actor_id.to_string()));
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
                port.provider = default_provider_for_port_name(port_name);
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
            "default_actor_id" => {
                port.default_actor_id = None;
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
            port.default_actor_id.as_ref(),
            &port.settings,
        )
        .await?;
        Ok(1)
    }

    pub async fn get_port_binding_record(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<(Uri, Uri)>> {
        Ok(self
            .get_port_binding_full_record(port, conversation_key)
            .await?
            .and_then(|(conversation_key, actor_id)| {
                actor_id.map(|actor_id| (conversation_key, actor_id))
            }))
    }

    pub async fn get_port_binding_full_record(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<(Uri, Option<Uri>)>> {
        let row = sqlx::query(
            r#"
            SELECT conversation_key, actor_id
            FROM port_bindings
            WHERE port = ?1 AND conversation_key = ?2
            LIMIT 1
            "#,
        )
        .bind(port)
        .bind(conversation_key.to_string())
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to query full port binding record")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let conversation_key_raw: String = row.try_get("conversation_key")?;
        let actor_id_raw: Option<String> = row.try_get("actor_id")?;
        Ok(Some((
            Uri::parse(&conversation_key_raw)?,
            actor_id_raw.map(|value| Uri::parse(&value)).transpose()?,
        )))
    }

    async fn upsert_port_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
        actor_update: ActorBindingUpdate<'_>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        match actor_update {
            ActorBindingUpdate::Preserve => {
                sqlx::query(
                    r#"
                    INSERT INTO port_bindings(
                        port,
                        conversation_key,
                        actor_id,
                        created_at,
                        updated_at
                    )
                    VALUES(?1, ?2, NULL, ?3, ?4)
                    ON CONFLICT(port, conversation_key) DO UPDATE SET
                      updated_at = excluded.updated_at
                    "#,
                )
                .bind(port)
                .bind(conversation_key.to_string())
                .bind(now.clone())
                .bind(now.clone())
                .execute(self.conn.pool())
                .await
                .context("failed to upsert port binding")?;
            }
            ActorBindingUpdate::Set(actor_id) => {
                sqlx::query(
                    r#"
                    INSERT INTO port_bindings(
                        port,
                        conversation_key,
                        actor_id,
                        created_at,
                        updated_at
                    )
                    VALUES(?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(port, conversation_key) DO UPDATE SET
                      actor_id = excluded.actor_id,
                      updated_at = excluded.updated_at
                    "#,
                )
                .bind(port)
                .bind(conversation_key.to_string())
                .bind(actor_id.map(ToString::to_string))
                .bind(now.clone())
                .bind(now.clone())
                .execute(self.conn.pool())
                .await
                .context("failed to upsert port binding")?;
            }
        }
        Ok(())
    }

    pub async fn upsert_port_binding_full_record(
        &self,
        port: &str,
        conversation_key: &Uri,
        actor_id: Option<Option<&Uri>>,
    ) -> Result<()> {
        let actor_update = match actor_id {
            Some(actor_id) => ActorBindingUpdate::Set(actor_id),
            None => ActorBindingUpdate::Preserve,
        };
        self.upsert_port_binding(port, conversation_key, actor_update)
            .await
    }

    pub async fn upsert_port_binding_record(
        &self,
        port: &str,
        conversation_key: &Uri,
        actor_id: &Uri,
    ) -> Result<()> {
        self.upsert_port_binding_full_record(port, conversation_key, Some(Some(actor_id)))
            .await
    }

    pub async fn list_port_binding_records(
        &self,
        port: &str,
        limit: usize,
    ) -> Result<Vec<(Uri, Option<Uri>)>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let rows = sqlx::query(
            r#"
            SELECT conversation_key, actor_id
            FROM port_bindings
            WHERE port = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
        )
        .bind(port)
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list full port bindings")?;

        rows.into_iter()
            .map(|row| {
                let conversation_key_raw: String = row.try_get("conversation_key")?;
                let actor_id_raw: Option<String> = row.try_get("actor_id")?;
                Ok((
                    Uri::parse(&conversation_key_raw)?,
                    actor_id_raw.map(|value| Uri::parse(&value)).transpose()?,
                ))
            })
            .collect()
    }

    pub async fn list_port_bindings(&self, port: &str, limit: usize) -> Result<Vec<(Uri, Uri)>> {
        Ok(self
            .list_port_binding_records(port, limit)
            .await?
            .into_iter()
            .filter_map(|(conversation_key, actor_id)| {
                actor_id.map(|actor_id| (conversation_key, actor_id))
            })
            .collect())
    }

    pub async fn delete_port_binding(&self, port: &str, conversation_key: &Uri) -> Result<u64> {
        let port = port.to_string();
        let conversation_key = conversation_key.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM port_bindings WHERE port = ?1 AND conversation_key = ?2",
            port,
            conversation_key,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to delete port binding")?
        .rows_affected();
        Ok(deleted)
    }

    pub async fn upsert_port_actor_context(
        &self,
        port: &str,
        actor_id: &Uri,
        ctx: &Value,
    ) -> Result<()> {
        let ctx_json = ctx.to_string();
        let ctx_json_ref = ctx_json.as_str();
        let now = Utc::now().to_rfc3339();
        let now_ref = now.as_str();
        let actor_id_raw = actor_id.to_string();
        let actor_id_ref = actor_id_raw.as_str();
        let updated = sqlx::query!(
            r#"
            UPDATE port_bindings
            SET context_snapshot_json = ?1, updated_at = ?2
            WHERE port = ?3
              AND actor_id = ?4
            "#,
            ctx_json_ref,
            now_ref,
            port,
            actor_id_ref,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to upsert port actor context snapshot")?
        .rows_affected();
        if updated == 0 {
            return Ok(());
        }
        Ok(())
    }

    pub async fn get_port_actor_context(
        &self,
        port: &str,
        actor_id: &Uri,
    ) -> Result<Option<Value>> {
        let actor_id_raw = actor_id.to_string();
        let row = sqlx::query!(
            r#"SELECT context_snapshot_json as "context_snapshot_json: String"
            FROM port_bindings
            WHERE port = ?1
              AND actor_id = ?2
              AND context_snapshot_json IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1"#,
            port,
            actor_id_raw,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to query actor context snapshot")?;

        let Some(row) = row else {
            return Ok(None);
        };
        let raw = row.context_snapshot_json;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let parsed = serde_json::from_str(&raw).context("invalid actor context snapshot json")?;
        Ok(Some(parsed))
    }

    pub async fn get_any_port_actor_context(
        &self,
        actor_id: &Uri,
    ) -> Result<Option<(String, Value)>> {
        let actor_id = actor_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                port as "port!: String",
                context_snapshot_json as "context_snapshot_json: String"
            FROM port_bindings
            WHERE actor_id = ?1
              AND context_snapshot_json IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1"#,
            actor_id
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to query actor context snapshot")?;

        let Some(row) = row else {
            return Ok(None);
        };
        let port_value = row.port;
        let raw = row.context_snapshot_json;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let parsed = serde_json::from_str(&raw).context("invalid actor context snapshot json")?;
        let port_name = port_value
            .strip_prefix("borg:port:")
            .unwrap_or(port_value.as_str())
            .to_string();
        Ok(Some((port_name, parsed)))
    }

    pub async fn list_port_actor_ids(&self, port: &str) -> Result<Vec<Uri>> {
        let port = port.to_string();
        let rows = sqlx::query!(
            r#"SELECT DISTINCT actor_id as "actor_id: String"
            FROM port_bindings
            WHERE port = ?1"#,
            port,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list port actor ids")?;

        rows.into_iter()
            .filter_map(|row| row.actor_id)
            .map(|row| Uri::parse(&row))
            .collect()
    }

    pub async fn clear_port_actor_context(&self, port: &str, actor_id: &Uri) -> Result<u64> {
        let actor_id = actor_id.to_string();
        let now = Utc::now().to_rfc3339();
        let deleted = sqlx::query(
            r#"
            UPDATE port_bindings
            SET context_snapshot_json = NULL, updated_at = ?1
            WHERE port = ?2
              AND actor_id = ?3
              AND context_snapshot_json IS NOT NULL
            "#,
        )
        .bind(now)
        .bind(port)
        .bind(actor_id)
        .execute(self.conn.pool())
        .await
        .context("failed to clear actor context snapshot")?
        .rows_affected();
        Ok(deleted)
    }

    pub async fn get_actor_reasoning_effort(&self, actor_id: &Uri) -> Result<Option<String>> {
        let actor_id = actor_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT current_reasoning_effort as "current_reasoning_effort?: String"
            FROM port_bindings
            WHERE actor_id = ?1
              AND current_reasoning_effort IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            actor_id
        )
        .fetch_optional(self.conn.pool())
        .await?;
        let value = row.and_then(|entry| entry.current_reasoning_effort);
        Ok(value
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty()))
    }

    pub async fn set_actor_reasoning_effort(
        &self,
        actor_id: &Uri,
        reasoning_effort: Option<&str>,
    ) -> Result<()> {
        let actor_id_raw = actor_id.to_string();
        let actor_id_ref = actor_id_raw.as_str();
        let now = Utc::now().to_rfc3339();
        let now_ref = now.as_str();
        let normalized = reasoning_effort
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let normalized_ref = normalized.as_deref();

        sqlx::query!(
            r#"
            UPDATE port_bindings
            SET current_reasoning_effort = ?1,
                updated_at = ?2
            WHERE actor_id = ?3
            "#,
            normalized_ref,
            now_ref,
            actor_id_ref
        )
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }
}

fn parse_port_enabled(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
}
