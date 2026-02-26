use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{BorgDb, ProviderRecord};

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

    pub async fn list_providers(&self, limit: usize) -> Result<Vec<ProviderRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut rows = self
            .conn
            .query(
                "SELECT provider, api_key, created_at, updated_at FROM providers ORDER BY provider ASC LIMIT ?1",
                (limit,),
            )
            .await
            .context("failed to list providers")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading provider row")? {
            let created_at: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            out.push(ProviderRecord {
                provider: row.get(0)?,
                api_key: row.get(1)?,
                created_at: parse_ts(&created_at)?,
                updated_at: parse_ts(&updated_at)?,
            });
        }
        Ok(out)
    }

    pub async fn get_provider(&self, provider: &str) -> Result<Option<ProviderRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT provider, api_key, created_at, updated_at FROM providers WHERE provider = ?1 LIMIT 1",
                (provider.to_string(),),
            )
            .await
            .context("failed to get provider")?;

        let Some(row) = rows.next().await.context("failed reading provider row")? else {
            return Ok(None);
        };

        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        Ok(Some(ProviderRecord {
            provider: row.get(0)?,
            api_key: row.get(1)?,
            created_at: parse_ts(&created_at)?,
            updated_at: parse_ts(&updated_at)?,
        }))
    }

    pub async fn delete_provider(&self, provider: &str) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM providers WHERE provider = ?1",
                (provider.to_string(),),
            )
            .await
            .context("failed to delete provider")?;
        Ok(deleted)
    }
}
