use anyhow::Result;
use chrono::Utc;
use serde_json::Value;

use borg_core::Uri;

use crate::{AgentSpecRecord, BorgDb};

impl BorgDb {
    pub async fn upsert_agent_spec(
        &self,
        agent_id: &Uri,
        model: &str,
        system_prompt: &str,
        tools: &Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO agent_specs(agent_id, model, system_prompt, tools_json, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(agent_id) DO UPDATE SET
                  model = excluded.model,
                  system_prompt = excluded.system_prompt,
                  tools_json = excluded.tools_json,
                  updated_at = excluded.updated_at
                "#,
                (
                    agent_id.to_string(),
                    model.to_string(),
                    system_prompt.to_string(),
                    tools.to_string(),
                    now.clone(),
                    now,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn get_agent_spec(&self, agent_id: &Uri) -> Result<Option<AgentSpecRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT agent_id, model, system_prompt, tools_json, updated_at FROM agent_specs WHERE agent_id = ?1 LIMIT 1",
                (agent_id.to_string(),),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let updated_at_raw: String = row.get(4)?;
        Ok(Some(AgentSpecRecord {
            agent_id: Uri::parse(&row.get::<String>(0)?)?,
            model: row.get(1)?,
            system_prompt: row.get(2)?,
            tools: serde_json::from_str(&row.get::<String>(3)?).unwrap_or(Value::Array(vec![])),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_raw)?.with_timezone(&Utc),
        }))
    }
}
