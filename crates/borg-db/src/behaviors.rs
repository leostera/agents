use anyhow::{Context, Result};
use borg_core::Uri;
use chrono::Utc;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{BehaviorRecord, BorgDb};

impl BorgDb {
    pub async fn upsert_behavior(
        &self,
        behavior_id: &Uri,
        name: &str,
        system_prompt: &str,
        preferred_provider_id: Option<&str>,
        required_capabilities_json: &serde_json::Value,
        session_turn_concurrency: &str,
        status: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO behaviors(
                behavior_id,
                name,
                system_prompt,
                preferred_provider_id,
                required_capabilities_json,
                session_turn_concurrency,
                status,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(behavior_id) DO UPDATE SET
              name = excluded.name,
              system_prompt = excluded.system_prompt,
              preferred_provider_id = excluded.preferred_provider_id,
              required_capabilities_json = excluded.required_capabilities_json,
              session_turn_concurrency = excluded.session_turn_concurrency,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(behavior_id.to_string())
        .bind(name)
        .bind(system_prompt)
        .bind(preferred_provider_id)
        .bind(required_capabilities_json.to_string())
        .bind(session_turn_concurrency)
        .bind(status)
        .bind(now.clone())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to upsert behavior")?;
        Ok(())
    }

    pub async fn list_behaviors(&self, limit: usize) -> Result<Vec<BehaviorRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT
                behavior_id,
                name,
                system_prompt,
                preferred_provider_id,
                required_capabilities_json,
                session_turn_concurrency,
                status,
                created_at,
                updated_at
            FROM behaviors
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .context("failed to list behaviors")?;
        rows.into_iter().map(behavior_from_row).collect()
    }

    pub async fn get_behavior(&self, behavior_id: &Uri) -> Result<Option<BehaviorRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                behavior_id,
                name,
                system_prompt,
                preferred_provider_id,
                required_capabilities_json,
                session_turn_concurrency,
                status,
                created_at,
                updated_at
            FROM behaviors
            WHERE behavior_id = ?1
            LIMIT 1
            "#,
        )
        .bind(behavior_id.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to get behavior")?;
        row.map(behavior_from_row).transpose()
    }

    pub async fn delete_behavior(&self, behavior_id: &Uri) -> Result<u64> {
        let deleted = sqlx::query("DELETE FROM behaviors WHERE behavior_id = ?1")
            .bind(behavior_id.to_string())
            .execute(self.pool())
            .await
            .context("failed to delete behavior")?
            .rows_affected();
        Ok(deleted)
    }
}

fn behavior_from_row(row: sqlx::sqlite::SqliteRow) -> Result<BehaviorRecord> {
    let required_capabilities_json_raw: String = row.try_get("required_capabilities_json")?;
    let required_capabilities_json = serde_json::from_str(&required_capabilities_json_raw)
        .context("invalid required_capabilities_json")?;
    Ok(BehaviorRecord {
        behavior_id: Uri::parse(&row.try_get::<String, _>("behavior_id")?)?,
        name: row.try_get("name")?,
        system_prompt: row.try_get("system_prompt")?,
        preferred_provider_id: row.try_get("preferred_provider_id")?,
        required_capabilities_json,
        session_turn_concurrency: row.try_get("session_turn_concurrency")?,
        status: row.try_get("status")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "borg-db-behaviors-{test_name}-{}.db",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[tokio::test]
    async fn behavior_crud_roundtrip() -> Result<()> {
        let path = tmp_db_path("crud");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let behavior_id = Uri::from_parts("borg", "behavior", Some("proto"))?;
        db.upsert_behavior(
            &behavior_id,
            "Prototyping",
            "You prototype quickly.",
            Some("lmstudio"),
            &serde_json::json!(["tool_calling"]),
            "serial",
            "ACTIVE",
        )
        .await?;

        let fetched = db.get_behavior(&behavior_id).await?.expect("behavior");
        assert_eq!(fetched.name, "Prototyping");
        assert_eq!(fetched.preferred_provider_id.as_deref(), Some("lmstudio"));

        let listed = db.list_behaviors(10).await?;
        assert_eq!(listed.len(), 1);

        let deleted = db.delete_behavior(&behavior_id).await?;
        assert_eq!(deleted, 1);
        Ok(())
    }
}
