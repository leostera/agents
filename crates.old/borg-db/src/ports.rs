use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, PortRecord};
use borg_core::{ActorId, PortId, WorkspaceId};

impl BorgDb {
    pub async fn list_ports(&self, limit: usize) -> Result<Vec<PortRecord>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let rows = sqlx::query!(
            r#"
            SELECT
                port_id as "port_id!: String",
                workspace_id as "workspace_id!: String",
                provider as "provider!: String",
                port_name as "port_name!: String",
                enabled as "enabled!: i64",
                allows_guests as "allows_guests!: i64",
                assigned_actor_id as "assigned_actor_id: String",
                settings_json as "settings_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM ports
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list ports")?;

        rows.into_iter()
            .map(|row| {
                Ok(PortRecord {
                    port_id: PortId::parse(&row.port_id)?,
                    workspace_id: WorkspaceId::from_id(&row.workspace_id),
                    provider: row.provider,
                    port_name: row.port_name,
                    enabled: row.enabled != 0,
                    allows_guests: row.allows_guests != 0,
                    assigned_actor_id: row
                        .assigned_actor_id
                        .map(|s| ActorId::parse(&s))
                        .transpose()?,
                    settings: serde_json::from_str(&row.settings_json)?,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_port(&self, port_name: &str) -> Result<Option<PortRecord>> {
        let name = port_name.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                port_id as "port_id!: String",
                workspace_id as "workspace_id!: String",
                provider as "provider!: String",
                port_name as "port_name!: String",
                enabled as "enabled!: i64",
                allows_guests as "allows_guests!: i64",
                assigned_actor_id as "assigned_actor_id: String",
                settings_json as "settings_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM ports
            WHERE port_name = ?1
            LIMIT 1
            "#,
            name,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get port")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(PortRecord {
            port_id: PortId::parse(&row.port_id)?,
            workspace_id: WorkspaceId::from_id(&row.workspace_id),
            provider: row.provider,
            port_name: row.port_name,
            enabled: row.enabled != 0,
            allows_guests: row.allows_guests != 0,
            assigned_actor_id: row
                .assigned_actor_id
                .map(|s| ActorId::parse(&s))
                .transpose()?,
            settings: serde_json::from_str(&row.settings_json)?,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn get_port_by_id(&self, port_id: &PortId) -> Result<Option<PortRecord>> {
        let id = port_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                port_id as "port_id!: String",
                workspace_id as "workspace_id!: String",
                provider as "provider!: String",
                port_name as "port_name!: String",
                enabled as "enabled!: i64",
                allows_guests as "allows_guests!: i64",
                assigned_actor_id as "assigned_actor_id: String",
                settings_json as "settings_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM ports
            WHERE port_id = ?1
            LIMIT 1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get port by id")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(PortRecord {
            port_id: PortId::parse(&row.port_id)?,
            workspace_id: WorkspaceId::from_id(&row.workspace_id),
            provider: row.provider,
            port_name: row.port_name,
            enabled: row.enabled != 0,
            allows_guests: row.allows_guests != 0,
            assigned_actor_id: row
                .assigned_actor_id
                .map(|s| ActorId::parse(&s))
                .transpose()?,
            settings: serde_json::from_str(&row.settings_json)?,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn upsert_port(
        &self,
        port_id: &PortId,
        workspace_id: &WorkspaceId,
        port_name: &str,
        provider: &str,
        enabled: bool,
        allows_guests: bool,
        assigned_actor_id: Option<&ActorId>,
        settings: &Value,
    ) -> Result<()> {
        let id = port_id.to_string();
        let workspace = workspace_id.to_string();
        let name = port_name.to_string();
        let prov = provider.to_string();
        let enabled_raw = if enabled { 1 } else { 0 };
        let allows_guests_raw = if allows_guests { 1 } else { 0 };
        let actor_id = assigned_actor_id.map(|id| id.to_string());
        let settings_json = serde_json::to_string(settings)?;
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO ports (
                port_id, workspace_id, provider, port_name,
                enabled, allows_guests, assigned_actor_id,
                settings_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(port_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                provider = excluded.provider,
                port_name = excluded.port_name,
                enabled = excluded.enabled,
                allows_guests = excluded.allows_guests,
                assigned_actor_id = excluded.assigned_actor_id,
                settings_json = excluded.settings_json,
                updated_at = excluded.updated_at
            "#,
            id,
            workspace,
            prov,
            name,
            enabled_raw,
            allows_guests_raw,
            actor_id,
            settings_json,
            now,
            now
        )
        .execute(self.pool())
        .await
        .context("failed to upsert port")?;

        Ok(())
    }

    pub async fn delete_port(&self, port_id: &PortId) -> Result<u64> {
        let id = port_id.to_string();
        let deleted = sqlx::query!("DELETE FROM ports WHERE port_id = ?1", id)
            .execute(self.pool())
            .await
            .context("failed to delete port")?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn upsert_port_setting(
        &self,
        workspace_id: &WorkspaceId,
        port_name: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let mut port = self.get_port(port_name).await?.ok_or_else(|| {
            anyhow::anyhow!("cannot update setting for non-existent port: {}", port_name)
        })?;

        match key {
            "provider" => {
                port.provider = value.trim().to_string();
            }
            "enabled" => {
                port.enabled = !matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                );
            }
            "allows_guests" => {
                port.allows_guests = !matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                );
            }
            "assigned_actor_id" => {
                let trimmed = value.trim();
                port.assigned_actor_id = if trimmed.is_empty() {
                    None
                } else {
                    Some(ActorId::parse(trimmed).context("invalid assigned_actor_id uri")?)
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
            &port.port_id,
            workspace_id,
            &port.port_name,
            &port.provider,
            port.enabled,
            port.allows_guests,
            port.assigned_actor_id.as_ref(),
            &port.settings,
        )
        .await
    }

    pub async fn get_port_setting(&self, port_name: &str, key: &str) -> Result<Option<String>> {
        let Some(port) = self.get_port(port_name).await? else {
            return Ok(None);
        };

        let value = match key {
            "provider" => Some(port.provider),
            "enabled" => Some(if port.enabled { "true" } else { "false" }.to_string()),
            "allows_guests" => Some(if port.allows_guests { "true" } else { "false" }.to_string()),
            "assigned_actor_id" => port.assigned_actor_id.map(|uri| uri.to_string()),
            _ => port
                .settings
                .as_object()
                .and_then(|map| map.get(key))
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        Ok(value)
    }
}
