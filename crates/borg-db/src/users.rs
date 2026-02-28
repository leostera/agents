use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use borg_core::Uri;

use crate::utils::parse_ts;
use crate::{BorgDb, UserRecord};

impl BorgDb {
    pub async fn upsert_user(&self, user_key: &Uri, profile: &Value) -> Result<()> {
        let user_key = user_key.to_string();
        let profile_json = profile.to_string();
        let now = Utc::now().to_rfc3339();
        let created_at = now.clone();
        let updated_at = now;
        sqlx::query!(
            r#"
            INSERT INTO users(user_key, profile_json, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4)
            ON CONFLICT(user_key) DO UPDATE SET
              profile_json = excluded.profile_json,
              updated_at = excluded.updated_at
            "#,
            user_key,
            profile_json,
            created_at,
            updated_at,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to upsert user")?;
        Ok(())
    }

    pub async fn list_users(&self, limit: usize) -> Result<Vec<UserRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"SELECT
                user_key as "user_key!: String",
                profile_json as "profile_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM users
            ORDER BY updated_at DESC
            LIMIT ?1"#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list users")?;

        rows.into_iter()
            .map(|row| {
                Ok(UserRecord {
                    user_key: Uri::parse(&row.user_key)?,
                    profile: serde_json::from_str(&row.profile_json).unwrap_or(Value::Null),
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_user(&self, user_key: &Uri) -> Result<Option<UserRecord>> {
        let user_key = user_key.to_string();
        let row = sqlx::query!(
            r#"SELECT
                user_key as "user_key!: String",
                profile_json as "profile_json!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM users
            WHERE user_key = ?1
            LIMIT 1"#,
            user_key,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get user")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(UserRecord {
            user_key: Uri::parse(&row.user_key)?,
            profile: serde_json::from_str(&row.profile_json).unwrap_or(Value::Null),
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn delete_user(&self, user_key: &Uri) -> Result<u64> {
        let user_key = user_key.to_string();
        let deleted = sqlx::query!("DELETE FROM users WHERE user_key = ?1", user_key)
            .execute(self.conn.pool())
            .await
            .context("failed to delete user")?
            .rows_affected();
        Ok(deleted)
    }
}
