use anyhow::Result;
use chrono::Utc;
use borg_core::Uri;

use crate::utils::parse_ts;
use crate::{AgentSpecRecord, BorgDb};

impl BorgDb {
    pub async fn upsert_agent_spec(
        &self,
        agent_id: &Uri,
        name: &str,
        default_provider_id: Option<&str>,
        model: &str,
        system_prompt: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO agent_specs(agent_id, name, default_provider_id, model, system_prompt, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(agent_id) DO UPDATE SET
              name = excluded.name,
              default_provider_id = excluded.default_provider_id,
              model = excluded.model,
              system_prompt = excluded.system_prompt,
              updated_at = excluded.updated_at
            "#,
            agent_id.to_string(),
            name.to_string(),
            default_provider_id.map(str::to_string),
            model.to_string(),
            system_prompt.to_string(),
            now.clone(),
            now,
        )
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn get_agent_spec(&self, agent_id: &Uri) -> Result<Option<AgentSpecRecord>> {
        let row = sqlx::query!(
            "SELECT agent_id, name, enabled, default_provider_id, model, system_prompt, updated_at FROM agent_specs WHERE agent_id = ?1 LIMIT 1",
            agent_id.to_string(),
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(AgentSpecRecord {
            agent_id: Uri::parse(&row.agent_id)?,
            name: row.name,
            enabled: row.enabled != 0,
            default_provider_id: row.default_provider_id,
            model: row.model,
            system_prompt: row.system_prompt,
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)?
                .with_timezone(&Utc),
        }))
    }

    pub async fn list_agent_specs(&self, limit: usize) -> Result<Vec<AgentSpecRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            "SELECT agent_id, name, enabled, default_provider_id, model, system_prompt, updated_at FROM agent_specs ORDER BY updated_at DESC LIMIT ?1",
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(AgentSpecRecord {
                    agent_id: Uri::parse(&row.agent_id)?,
                    name: row.name,
                    enabled: row.enabled != 0,
                    default_provider_id: row.default_provider_id,
                    model: row.model,
                    system_prompt: row.system_prompt,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn set_agent_spec_enabled(&self, agent_id: &Uri, enabled: bool) -> Result<u64> {
        let updated = sqlx::query!(
            "UPDATE agent_specs SET enabled = ?2, updated_at = ?3 WHERE agent_id = ?1",
            agent_id.to_string(),
            if enabled { 1_i64 } else { 0_i64 },
            Utc::now().to_rfc3339(),
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(updated)
    }

    pub async fn delete_agent_spec(&self, agent_id: &Uri) -> Result<u64> {
        let deleted = sqlx::query!(
            "DELETE FROM agent_specs WHERE agent_id = ?1",
            agent_id.to_string(),
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(deleted)
    }
}
