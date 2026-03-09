use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{BorgDb, PortBindingRecord};
use borg_core::{ActorId, PortId, WorkspaceId};

impl BorgDb {
    pub async fn upsert_port_binding(
        &self,
        workspace_id: &WorkspaceId,
        port_id: &PortId,
        conversation_key: &str,
        actor_id: &ActorId,
    ) -> Result<()> {
        let workspace = workspace_id.to_string();
        let port = port_id.to_string();
        let key = conversation_key.to_string();
        let actor = actor_id.to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO port_bindings (
                workspace_id, port_id, conversation_key, actor_id,
                created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(port_id, conversation_key) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                actor_id = excluded.actor_id,
                updated_at = excluded.updated_at
            "#,
            workspace,
            port,
            key,
            actor,
            now,
            now
        )
        .execute(self.pool())
        .await
        .context("failed to upsert port binding")?;

        Ok(())
    }

    pub async fn get_port_binding(
        &self,
        port_id: &PortId,
        conversation_key: &str,
    ) -> Result<Option<PortBindingRecord>> {
        let port = port_id.to_string();
        let key = conversation_key.to_string();

        let row = sqlx::query!(
            r#"
            SELECT
                workspace_id as "workspace_id!: String",
                port_id as "port_id!: String",
                conversation_key as "conversation_key!: String",
                actor_id as "actor_id!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM port_bindings
            WHERE port_id = ?1 AND conversation_key = ?2
            LIMIT 1
            "#,
            port,
            key
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get port binding")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(PortBindingRecord {
            workspace_id: WorkspaceId::parse(&row.workspace_id)?,
            port_id: PortId::parse(&row.port_id)?,
            conversation_key: row.conversation_key,
            actor_id: ActorId::parse(&row.actor_id)?,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn list_port_bindings(
        &self,
        port_id: &PortId,
        limit: usize,
    ) -> Result<Vec<PortBindingRecord>> {
        let port = port_id.to_string();
        let limit = i64::try_from(limit).unwrap_or(100);

        let rows = sqlx::query!(
            r#"
            SELECT
                workspace_id as "workspace_id!: String",
                port_id as "port_id!: String",
                conversation_key as "conversation_key!: String",
                actor_id as "actor_id!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM port_bindings
            WHERE port_id = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
            port,
            limit
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list port bindings")?;

        rows.into_iter()
            .map(|row| {
                Ok(PortBindingRecord {
                    workspace_id: WorkspaceId::parse(&row.workspace_id)?,
                    port_id: PortId::parse(&row.port_id)?,
                    conversation_key: row.conversation_key,
                    actor_id: ActorId::parse(&row.actor_id)?,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn delete_port_binding(
        &self,
        port_id: &PortId,
        conversation_key: &str,
    ) -> Result<u64> {
        let port = port_id.to_string();
        let key = conversation_key.to_string();

        let deleted = sqlx::query!(
            "DELETE FROM port_bindings WHERE port_id = ?1 AND conversation_key = ?2",
            port,
            key
        )
        .execute(self.pool())
        .await
        .context("failed to delete port binding")?
        .rows_affected();

        Ok(deleted)
    }

    /// Finds any port conversation bound to this actor and returns (port_name, settings_json).
    pub async fn get_any_port_actor_context(
        &self,
        actor_id: &ActorId,
    ) -> Result<Option<(String, String)>> {
        let actor = actor_id.to_string();

        let row = sqlx::query!(
            r#"
            SELECT
                p.port_name as "port_name!: String",
                p.settings_json as "settings_json!: String"
            FROM port_bindings pb
            JOIN ports p ON pb.port_id = p.port_id
            WHERE pb.actor_id = ?1
            LIMIT 1
            "#,
            actor
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get any port actor context")?;

        Ok(row.map(|r| (r.port_name, r.settings_json)))
    }

    /// Resolves the actor for a given port conversation.
    /// If no binding exists, it returns None.
    pub async fn resolve_port_actor(
        &self,
        port_id: &PortId,
        conversation_key: &str,
    ) -> Result<Option<ActorId>> {
        if let Some(existing) = self.get_port_binding(port_id, conversation_key).await? {
            return Ok(Some(existing.actor_id));
        }
        Ok(None)
    }
}
