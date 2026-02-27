use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use borg_core::Uri;

use crate::utils::parse_ts;
use crate::{BorgDb, PolicyRecord, PolicyUseRecord};

impl BorgDb {
    pub async fn upsert_policy(&self, policy_id: &Uri, policy: &Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO policies(policy_id, policy_json, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4)
                ON CONFLICT(policy_id) DO UPDATE SET
                  policy_json = excluded.policy_json,
                  updated_at = excluded.updated_at
                "#,
                (policy_id.to_string(), policy.to_string(), now.clone(), now),
            )
            .await
            .context("failed to upsert policy")?;
        Ok(())
    }

    pub async fn list_policies(&self, limit: usize) -> Result<Vec<PolicyRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut rows = self
            .conn
            .query(
                "SELECT policy_id, policy_json, created_at, updated_at FROM policies ORDER BY updated_at DESC LIMIT ?1",
                (limit,),
            )
            .await
            .context("failed to list policies")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading policy row")? {
            let created_at: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            out.push(PolicyRecord {
                policy_id: Uri::parse(&row.get::<String>(0)?)?,
                policy: serde_json::from_str(&row.get::<String>(1)?).unwrap_or(Value::Null),
                created_at: parse_ts(&created_at)?,
                updated_at: parse_ts(&updated_at)?,
            });
        }
        Ok(out)
    }

    pub async fn get_policy(&self, policy_id: &Uri) -> Result<Option<PolicyRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT policy_id, policy_json, created_at, updated_at FROM policies WHERE policy_id = ?1 LIMIT 1",
                (policy_id.to_string(),),
            )
            .await
            .context("failed to get policy")?;

        let Some(row) = rows.next().await.context("failed reading policy row")? else {
            return Ok(None);
        };

        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        Ok(Some(PolicyRecord {
            policy_id: Uri::parse(&row.get::<String>(0)?)?,
            policy: serde_json::from_str(&row.get::<String>(1)?).unwrap_or(Value::Null),
            created_at: parse_ts(&created_at)?,
            updated_at: parse_ts(&updated_at)?,
        }))
    }

    pub async fn delete_policy(&self, policy_id: &Uri) -> Result<u64> {
        self.conn
            .execute(
                "DELETE FROM policies_use WHERE policy_id = ?1",
                (policy_id.to_string(),),
            )
            .await
            .context("failed to delete policy associations")?;
        let deleted = self
            .conn
            .execute(
                "DELETE FROM policies WHERE policy_id = ?1",
                (policy_id.to_string(),),
            )
            .await
            .context("failed to delete policy")?;
        Ok(deleted)
    }

    pub async fn attach_policy_to_entity(&self, policy_id: &Uri, entity_id: &Uri) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT OR IGNORE INTO policies_use(policy_id, entity_id, created_at) VALUES(?1, ?2, ?3)",
                (policy_id.to_string(), entity_id.to_string(), now),
            )
            .await
            .context("failed to attach policy to entity")?;
        Ok(())
    }

    pub async fn list_policy_uses(
        &self,
        policy_id: &Uri,
        limit: usize,
    ) -> Result<Vec<PolicyUseRecord>> {
        let limit = i64::try_from(limit).unwrap_or(200);
        let mut rows = self
            .conn
            .query(
                "SELECT policy_id, entity_id, created_at FROM policies_use WHERE policy_id = ?1 ORDER BY created_at DESC LIMIT ?2",
                (policy_id.to_string(), limit),
            )
            .await
            .context("failed to list policy uses")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading policy use row")? {
            let created_at: String = row.get(2)?;
            out.push(PolicyUseRecord {
                policy_id: Uri::parse(&row.get::<String>(0)?)?,
                entity_id: Uri::parse(&row.get::<String>(1)?)?,
                created_at: parse_ts(&created_at)?,
            });
        }
        Ok(out)
    }

    pub async fn detach_policy_from_entity(&self, policy_id: &Uri, entity_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM policies_use WHERE policy_id = ?1 AND entity_id = ?2",
                (policy_id.to_string(), entity_id.to_string()),
            )
            .await
            .context("failed to detach policy from entity")?;
        Ok(deleted)
    }
}
