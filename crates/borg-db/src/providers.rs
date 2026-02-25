use anyhow::{Context, Result};
use chrono::Utc;

use crate::BorgDb;

impl BorgDb {
    pub async fn upsert_provider_api_key(&self, provider: &str, api_key: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO providers(provider, api_key, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4)
                ON CONFLICT(provider) DO UPDATE SET
                  api_key = excluded.api_key,
                  updated_at = excluded.updated_at
                "#,
                (provider.to_string(), api_key.to_string(), now.clone(), now),
            )
            .await
            .context("failed to upsert provider api key")?;
        Ok(())
    }

    pub async fn get_provider_api_key(&self, provider: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT api_key FROM providers WHERE provider = ?1 LIMIT 1",
                (provider.to_string(),),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        Ok(Some(row.get(0)?))
    }
}
