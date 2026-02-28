use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use borg_core::Uri;

use crate::utils::parse_ts;
use crate::{BorgDb, PolicyRecord, PolicyUseRecord};

impl BorgDb {
    pub async fn upsert_policy(&self, policy_id: &Uri, policy: &Value) -> Result<()> {
        let policy_id = policy_id.to_string();
        let policy_json = policy.to_string();
        let now = Utc::now().to_rfc3339();
        let created_at = now.clone();
        let updated_at = now;
        sqlx::query!(
            r#"
            INSERT INTO policies(policy_id, policy_json, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4)
            ON CONFLICT(policy_id) DO UPDATE SET
              policy_json = excluded.policy_json,
              updated_at = excluded.updated_at
            "#,
            policy_id,
            policy_json,
            created_at,
            updated_at,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to upsert policy")?;
        Ok(())
    }

    pub async fn list_policies(&self, limit: usize) -> Result<Vec<PolicyRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"SELECT
                policy_id as "policy_id!: String",
                policy_json as "policy_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM policies
            ORDER BY updated_at DESC
            LIMIT ?1"#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list policies")?;

        rows.into_iter()
            .map(|row| {
                Ok(PolicyRecord {
                    policy_id: Uri::parse(&row.policy_id)?,
                    policy: serde_json::from_str(&row.policy_json).unwrap_or(Value::Null),
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_policy(&self, policy_id: &Uri) -> Result<Option<PolicyRecord>> {
        let policy_id = policy_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                policy_id as "policy_id!: String",
                policy_json as "policy_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM policies
            WHERE policy_id = ?1
            LIMIT 1"#,
            policy_id,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get policy")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(PolicyRecord {
            policy_id: Uri::parse(&row.policy_id)?,
            policy: serde_json::from_str(&row.policy_json).unwrap_or(Value::Null),
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn delete_policy(&self, policy_id: &Uri) -> Result<u64> {
        let policy_id = policy_id.to_string();
        let policy_id_for_use = policy_id.clone();
        sqlx::query!(
            "DELETE FROM policies_use WHERE policy_id = ?1",
            policy_id_for_use,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to delete policy associations")?;
        let deleted = sqlx::query!("DELETE FROM policies WHERE policy_id = ?1", policy_id,)
            .execute(self.conn.pool())
            .await
            .context("failed to delete policy")?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn attach_policy_to_entity(&self, policy_id: &Uri, entity_id: &Uri) -> Result<()> {
        let policy_id = policy_id.to_string();
        let entity_id = entity_id.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            "INSERT OR IGNORE INTO policies_use(policy_id, entity_id, created_at) VALUES(?1, ?2, ?3)",
            policy_id,
            entity_id,
            now,
        )
            .execute(self.conn.pool())
            .await
            .context("failed to attach policy to entity")?;
        Ok(())
    }

    pub async fn list_policy_uses(
        &self,
        policy_id: &Uri,
        limit: usize,
    ) -> Result<Vec<PolicyUseRecord>> {
        let policy_id = policy_id.to_string();
        let limit = i64::try_from(limit).unwrap_or(200);
        let rows = sqlx::query!(
            r#"SELECT
                policy_id as "policy_id!: String",
                entity_id as "entity_id!: String",
                created_at as "created_at!: String"
            FROM policies_use
            WHERE policy_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2"#,
            policy_id,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list policy uses")?;

        rows.into_iter()
            .map(|row| {
                Ok(PolicyUseRecord {
                    policy_id: Uri::parse(&row.policy_id)?,
                    entity_id: Uri::parse(&row.entity_id)?,
                    created_at: parse_ts(&row.created_at)?,
                })
            })
            .collect()
    }

    pub async fn detach_policy_from_entity(&self, policy_id: &Uri, entity_id: &Uri) -> Result<u64> {
        let policy_id = policy_id.to_string();
        let entity_id = entity_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM policies_use WHERE policy_id = ?1 AND entity_id = ?2",
            policy_id,
            entity_id,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to detach policy from entity")?
        .rows_affected();
        Ok(deleted)
    }
}
