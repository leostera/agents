use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{BorgDb, ProviderRecord};

impl BorgDb {
    pub async fn upsert_provider(
        &self,
        provider: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        enabled: Option<bool>,
        default_text_model: Option<&str>,
        default_audio_model: Option<&str>,
    ) -> Result<()> {
        self.upsert_provider_with_kind(
            provider,
            provider,
            api_key,
            base_url,
            enabled,
            default_text_model,
            default_audio_model,
        )
        .await
    }

    pub async fn upsert_provider_with_kind(
        &self,
        provider: &str,
        provider_kind: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        enabled: Option<bool>,
        default_text_model: Option<&str>,
        default_audio_model: Option<&str>,
    ) -> Result<()> {
        let provider = provider.to_string();
        let provider_kind = provider_kind.to_string();
        let api_key = api_key.map(|value| value.to_string());
        let base_url = base_url.map(|value| value.to_string());
        let enabled_raw = enabled.map(|value| if value { 1_i64 } else { 0_i64 });
        let default_text_model = default_text_model.map(ToString::to_string);
        let default_audio_model = default_audio_model.map(ToString::to_string);
        let now = Utc::now().to_rfc3339();
        let created_at = now.clone();
        let updated_at = now;
        sqlx::query!(
            r#"
            INSERT INTO providers(
                provider,
                provider_kind,
                api_key,
                base_url,
                enabled,
                default_text_model,
                default_audio_model,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, COALESCE(?3, ''), ?4, COALESCE(?5, 1), ?6, ?7, ?8, ?9)
            ON CONFLICT(provider) DO UPDATE SET
              provider_kind = COALESCE(?2, providers.provider_kind),
              api_key = COALESCE(?3, providers.api_key),
              base_url = COALESCE(?4, providers.base_url),
              enabled = COALESCE(?5, providers.enabled),
              default_text_model = COALESCE(?6, providers.default_text_model),
              default_audio_model = COALESCE(?7, providers.default_audio_model),
              updated_at = excluded.updated_at
            "#,
            provider,
            provider_kind,
            api_key,
            base_url,
            enabled_raw,
            default_text_model,
            default_audio_model,
            created_at,
            updated_at,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to upsert provider")?;
        Ok(())
    }

    pub async fn upsert_provider_api_key(&self, provider: &str, api_key: &str) -> Result<()> {
        self.upsert_provider(provider, Some(api_key), None, None, None, None)
            .await
    }

    pub async fn get_provider_api_key(&self, provider: &str) -> Result<Option<String>> {
        let provider = provider.to_string();
        let row = sqlx::query!(
            r#"SELECT api_key as "api_key!: String"
            FROM providers
            WHERE provider = ?1 AND enabled = 1
            LIMIT 1"#,
            provider
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let api_key = row.api_key.trim().to_string();
        if api_key.is_empty() {
            return Ok(None);
        }

        Ok(Some(api_key))
    }

    pub async fn list_providers(&self, limit: usize) -> Result<Vec<ProviderRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"SELECT
                p.provider as "provider!: String",
                COALESCE(NULLIF(TRIM(p.provider_kind), ''), p.provider) as "provider_kind!: String",
                p.api_key as "api_key!: String",
                p.base_url as "base_url: String",
                p.enabled as "enabled!: i64",
                COALESCE(s.tokens_used, 0) as "tokens_used!: i64",
                s.last_used as "last_used: String",
                p.default_text_model as "default_text_model: String",
                p.default_audio_model as "default_audio_model: String",
                p.created_at as "created_at!: String",
                p.updated_at as "updated_at!: String"
            FROM providers p
            LEFT JOIN provider_usage_summaries s ON s.provider = p.provider
            ORDER BY p.provider ASC
            LIMIT ?1"#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list providers")?;

        rows.into_iter()
            .map(|row| {
                Ok(ProviderRecord {
                    provider: row.provider,
                    provider_kind: row.provider_kind,
                    api_key: row.api_key,
                    base_url: row.base_url,
                    enabled: row.enabled != 0,
                    tokens_used: u64::try_from(row.tokens_used).unwrap_or(0),
                    last_used: row.last_used.as_deref().map(parse_ts).transpose()?,
                    default_text_model: row.default_text_model,
                    default_audio_model: row.default_audio_model,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_provider(&self, provider: &str) -> Result<Option<ProviderRecord>> {
        let provider = provider.to_string();
        let row = sqlx::query!(
            r#"SELECT
                p.provider as "provider!: String",
                COALESCE(NULLIF(TRIM(p.provider_kind), ''), p.provider) as "provider_kind!: String",
                p.api_key as "api_key!: String",
                p.base_url as "base_url: String",
                p.enabled as "enabled!: i64",
                COALESCE(s.tokens_used, 0) as "tokens_used!: i64",
                s.last_used as "last_used: String",
                p.default_text_model as "default_text_model: String",
                p.default_audio_model as "default_audio_model: String",
                p.created_at as "created_at!: String",
                p.updated_at as "updated_at!: String"
            FROM providers p
            LEFT JOIN provider_usage_summaries s ON s.provider = p.provider
            WHERE p.provider = ?1
            LIMIT 1"#,
            provider,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get provider")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(ProviderRecord {
            provider: row.provider,
            provider_kind: row.provider_kind,
            api_key: row.api_key,
            base_url: row.base_url,
            enabled: row.enabled != 0,
            tokens_used: u64::try_from(row.tokens_used).unwrap_or(0),
            last_used: row.last_used.as_deref().map(parse_ts).transpose()?,
            default_text_model: row.default_text_model,
            default_audio_model: row.default_audio_model,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn record_provider_usage(&self, provider: &str, tokens_used: u64) -> Result<()> {
        let provider = provider.to_string();
        let now = Utc::now().to_rfc3339();
        let tokens_used_raw = i64::try_from(tokens_used).unwrap_or(i64::MAX);
        sqlx::query!(
            r#"
            INSERT INTO provider_usage_summaries(provider, tokens_used, last_used)
            VALUES(?1, ?2, ?3)
            ON CONFLICT(provider) DO UPDATE SET
              tokens_used = provider_usage_summaries.tokens_used + excluded.tokens_used,
              last_used = excluded.last_used
            "#,
            provider,
            tokens_used_raw,
            now
        )
        .execute(self.conn.pool())
        .await
        .context("failed to record provider usage")?;
        Ok(())
    }

    pub async fn delete_provider(&self, provider: &str) -> Result<u64> {
        let provider = provider.to_string();
        let provider_for_usage = provider.clone();
        sqlx::query!(
            "DELETE FROM provider_usage_summaries WHERE provider = ?1",
            provider_for_usage,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to delete provider usage summary")?;
        let deleted = sqlx::query!("DELETE FROM providers WHERE provider = ?1", provider,)
            .execute(self.conn.pool())
            .await
            .context("failed to delete provider")?
            .rows_affected();
        Ok(deleted)
    }
}
