use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{ActorRecord, BorgDb};
use borg_core::{ActorId, ProviderId, WorkspaceId};

impl BorgDb {
    pub async fn upsert_actor(
        &self,
        actor_id: &ActorId,
        workspace_id: &WorkspaceId,
        name: &str,
        system_prompt: &str,
        actor_prompt: &str,
        status: &str,
    ) -> Result<()> {
        let actor_id = actor_id.to_string();
        let workspace_id = workspace_id.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO actors(
                actor_id,
                workspace_id,
                name,
                system_prompt,
                actor_prompt,
                status,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(actor_id) DO UPDATE SET
              workspace_id = excluded.workspace_id,
              name = excluded.name,
              system_prompt = excluded.system_prompt,
              actor_prompt = excluded.actor_prompt,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
            actor_id,
            workspace_id,
            name,
            system_prompt,
            actor_prompt,
            status,
            now,
            now,
        )
        .execute(self.pool())
        .await
        .context("failed to upsert actor")?;
        Ok(())
    }

    pub async fn get_actor(&self, actor_id: &ActorId) -> Result<Option<ActorRecord>> {
        let id = actor_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                actor_id as "actor_id!: String",
                workspace_id as "workspace_id!: String",
                name as "name!: String",
                system_prompt as "system_prompt!: String",
                actor_prompt as "actor_prompt!: String",
                default_provider_id as "default_provider_id: String",
                model as "model: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM actors
            WHERE actor_id = ?1
            LIMIT 1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get actor")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(ActorRecord {
            actor_id: ActorId::parse(&row.actor_id)?,
            workspace_id: WorkspaceId::parse(&row.workspace_id)?,
            name: row.name,
            system_prompt: row.system_prompt,
            actor_prompt: row.actor_prompt,
            default_provider_id: row
                .default_provider_id
                .map(|id| ProviderId::parse(&id))
                .transpose()?,
            model: row.model,
            status: row.status,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn list_actors(&self, limit: usize) -> Result<Vec<ActorRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"
            SELECT
                actor_id as "actor_id!: String",
                workspace_id as "workspace_id!: String",
                name as "name!: String",
                system_prompt as "system_prompt!: String",
                actor_prompt as "actor_prompt!: String",
                default_provider_id as "default_provider_id: String",
                model as "model: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM actors
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list actors")?;

        rows.into_iter()
            .map(|row| {
                Ok(ActorRecord {
                    actor_id: ActorId::parse(&row.actor_id)?,
                    workspace_id: WorkspaceId::parse(&row.workspace_id)?,
                    name: row.name,
                    system_prompt: row.system_prompt,
                    actor_prompt: row.actor_prompt,
                    default_provider_id: row
                        .default_provider_id
                        .map(|id| ProviderId::parse(&id))
                        .transpose()?,
                    model: row.model,
                    status: row.status,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn set_actor_model(&self, actor_id: &ActorId, model: &str) -> Result<u64> {
        let id = actor_id.to_string();
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query!(
            r#"
            UPDATE actors
            SET model = ?2,
                updated_at = ?3
            WHERE actor_id = ?1
            "#,
            id,
            model,
            now,
        )
        .execute(self.pool())
        .await
        .context("failed to update actor model")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn delete_actor(&self, actor_id: &ActorId) -> Result<u64> {
        let id = actor_id.to_string();
        let deleted = sqlx::query!("DELETE FROM actors WHERE actor_id = ?1", id,)
            .execute(self.pool())
            .await
            .context("failed to delete actor")?
            .rows_affected();
        Ok(deleted)
    }
}
