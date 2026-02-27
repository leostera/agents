use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use borg_core::Uri;

use crate::utils::parse_ts;
use crate::{BorgDb, UserRecord};

impl BorgDb {
    pub async fn upsert_user(&self, user_key: &Uri, profile: &Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO users(user_key, profile_json, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4)
                ON CONFLICT(user_key) DO UPDATE SET
                  profile_json = excluded.profile_json,
                  updated_at = excluded.updated_at
                "#,
                (user_key.to_string(), profile.to_string(), now.clone(), now),
            )
            .await
            .context("failed to upsert user")?;
        Ok(())
    }

    pub async fn list_users(&self, limit: usize) -> Result<Vec<UserRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut rows = self
            .conn
            .query(
                "SELECT user_key, profile_json, created_at, updated_at FROM users ORDER BY updated_at DESC LIMIT ?1",
                (limit,),
            )
            .await
            .context("failed to list users")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading user row")? {
            let created_at: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            out.push(UserRecord {
                user_key: Uri::parse(&row.get::<String>(0)?)?,
                profile: serde_json::from_str(&row.get::<String>(1)?).unwrap_or(Value::Null),
                created_at: parse_ts(&created_at)?,
                updated_at: parse_ts(&updated_at)?,
            });
        }
        Ok(out)
    }

    pub async fn get_user(&self, user_key: &Uri) -> Result<Option<UserRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT user_key, profile_json, created_at, updated_at FROM users WHERE user_key = ?1 LIMIT 1",
                (user_key.to_string(),),
            )
            .await
            .context("failed to get user")?;

        let Some(row) = rows.next().await.context("failed reading user row")? else {
            return Ok(None);
        };

        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        Ok(Some(UserRecord {
            user_key: Uri::parse(&row.get::<String>(0)?)?,
            profile: serde_json::from_str(&row.get::<String>(1)?).unwrap_or(Value::Null),
            created_at: parse_ts(&created_at)?,
            updated_at: parse_ts(&updated_at)?,
        }))
    }

    pub async fn delete_user(&self, user_key: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM users WHERE user_key = ?1",
                (user_key.to_string(),),
            )
            .await
            .context("failed to delete user")?;
        Ok(deleted)
    }
}
