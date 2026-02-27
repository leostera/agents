use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{BorgDb, ProviderRecord};

impl BorgDb {
    pub async fn upsert_provider(
        &self,
        provider: &str,
        api_key: &str,
        enabled: Option<bool>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let enabled = enabled.map(|value| if value { 1_i64 } else { 0_i64 });
        self.conn
            .execute(
                r#"
                INSERT INTO providers(provider, api_key, enabled, created_at, updated_at)
                VALUES(?1, ?2, COALESCE(?3, 1), ?4, ?5)
                ON CONFLICT(provider) DO UPDATE SET
                  api_key = excluded.api_key,
                  enabled = COALESCE(?3, providers.enabled),
                  updated_at = excluded.updated_at
                "#,
                (
                    provider.to_string(),
                    api_key.to_string(),
                    enabled,
                    now.clone(),
                    now,
                ),
            )
            .await
            .context("failed to upsert provider")?;
        Ok(())
    }

    pub async fn upsert_provider_api_key(&self, provider: &str, api_key: &str) -> Result<()> {
        self.upsert_provider(provider, api_key, None).await
    }

    pub async fn get_provider_api_key(&self, provider: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT api_key FROM providers WHERE provider = ?1 AND enabled = 1 LIMIT 1",
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
                r#"
                SELECT
                    p.provider,
                    p.api_key,
                    p.enabled,
                    COALESCE(s.tokens_used, 0),
                    s.last_used,
                    p.created_at,
                    p.updated_at
                FROM providers p
                LEFT JOIN provider_usage_summaries s ON s.provider = p.provider
                ORDER BY p.provider ASC
                LIMIT ?1
                "#,
                (limit,),
            )
            .await
            .context("failed to list providers")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading provider row")? {
            let enabled_raw: i64 = row.get(2)?;
            let tokens_used_raw: i64 = row.get(3)?;
            let last_used_raw: Option<String> = row.get(4)?;
            let created_at: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            out.push(ProviderRecord {
                provider: row.get(0)?,
                api_key: row.get(1)?,
                enabled: enabled_raw != 0,
                tokens_used: u64::try_from(tokens_used_raw).unwrap_or(0),
                last_used: last_used_raw.as_deref().map(parse_ts).transpose()?,
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
                r#"
                SELECT
                    p.provider,
                    p.api_key,
                    p.enabled,
                    COALESCE(s.tokens_used, 0),
                    s.last_used,
                    p.created_at,
                    p.updated_at
                FROM providers p
                LEFT JOIN provider_usage_summaries s ON s.provider = p.provider
                WHERE p.provider = ?1
                LIMIT 1
                "#,
                (provider.to_string(),),
            )
            .await
            .context("failed to get provider")?;

        let Some(row) = rows.next().await.context("failed reading provider row")? else {
            return Ok(None);
        };

        let enabled_raw: i64 = row.get(2)?;
        let tokens_used_raw: i64 = row.get(3)?;
        let last_used_raw: Option<String> = row.get(4)?;
        let created_at: String = row.get(5)?;
        let updated_at: String = row.get(6)?;
        Ok(Some(ProviderRecord {
            provider: row.get(0)?,
            api_key: row.get(1)?,
            enabled: enabled_raw != 0,
            tokens_used: u64::try_from(tokens_used_raw).unwrap_or(0),
            last_used: last_used_raw.as_deref().map(parse_ts).transpose()?,
            created_at: parse_ts(&created_at)?,
            updated_at: parse_ts(&updated_at)?,
        }))
    }

    pub async fn record_provider_usage(&self, provider: &str, tokens_used: u64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let tokens_used = i64::try_from(tokens_used).unwrap_or(i64::MAX);
        self.conn
            .execute(
                r#"
                INSERT INTO provider_usage_summaries(provider, tokens_used, last_used)
                VALUES(?1, ?2, ?3)
                ON CONFLICT(provider) DO UPDATE SET
                  tokens_used = provider_usage_summaries.tokens_used + excluded.tokens_used,
                  last_used = excluded.last_used
                "#,
                (provider.to_string(), tokens_used, now),
            )
            .await
            .context("failed to record provider usage")?;
        Ok(())
    }

    pub async fn delete_provider(&self, provider: &str) -> Result<u64> {
        self.conn
            .execute(
                "DELETE FROM provider_usage_summaries WHERE provider = ?1",
                (provider.to_string(),),
            )
            .await
            .context("failed to delete provider usage summary")?;
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
