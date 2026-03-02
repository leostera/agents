use anyhow::{Context, Result, anyhow};
use borg_core::Uri;
use chrono::Utc;
use sqlx::Row;

use crate::BorgDb;

impl BorgDb {
    pub async fn upsert_port_actor_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
        actor_id: &Uri,
    ) -> Result<()> {
        if self.get_actor(actor_id).await?.is_none() {
            return Err(anyhow!("actor spec not found for actor_id {}", actor_id));
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO port_bindings(
                port,
                conversation_key,
                session_id,
                agent_id,
                actor_id,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, NULL, ?4, ?5, ?6)
            ON CONFLICT(port, conversation_key) DO UPDATE SET
              actor_id = excluded.actor_id,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(port)
        .bind(conversation_key.to_string())
        .bind(conversation_key.to_string())
        .bind(actor_id.to_string())
        .bind(now.clone())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to upsert port actor binding")?;
        Ok(())
    }

    pub async fn get_port_actor_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<Uri>> {
        let row = sqlx::query(
            r#"
            SELECT actor_id
            FROM port_bindings
            WHERE port = ?1 AND conversation_key = ?2
            LIMIT 1
            "#,
        )
        .bind(port)
        .bind(conversation_key.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to fetch port actor binding")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let actor_id = row.try_get::<Option<String>, _>("actor_id")?;
        actor_id.map(|value| Uri::parse(&value)).transpose()
    }

    pub async fn resolve_port_actor(
        &self,
        port: &str,
        conversation_key: &Uri,
        requested_actor_id: Option<&Uri>,
        default_actor_id: Option<&Uri>,
    ) -> Result<Uri> {
        if let Some(actor_id) = requested_actor_id {
            self.upsert_port_actor_binding(port, conversation_key, actor_id)
                .await?;
            return Ok(actor_id.clone());
        }

        if let Some(existing) = self.get_port_actor_binding(port, conversation_key).await? {
            return Ok(existing);
        }

        if let Some(default_actor_id) = default_actor_id {
            self.upsert_port_actor_binding(port, conversation_key, default_actor_id)
                .await?;
            return Ok(default_actor_id.clone());
        }

        Err(anyhow!(
            "no actor binding for port={} conversation_key={}",
            port,
            conversation_key
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "borg-db-actor-bindings-{test_name}-{}.db",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[tokio::test]
    async fn resolve_port_actor_prefers_requested_then_existing() -> Result<()> {
        let path = tmp_db_path("resolve");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor_a = Uri::from_parts("devmode", "actor", Some("bind-a"))?;
        let actor_b = Uri::from_parts("devmode", "actor", Some("bind-b"))?;
        let key = Uri::from_parts("borg", "conversation", Some("c1"))?;
        db.upsert_actor(&actor_a, "A", "prompt", "RUNNING").await?;
        db.upsert_actor(&actor_b, "B", "prompt", "RUNNING").await?;

        let resolved = db
            .resolve_port_actor("telegram", &key, Some(&actor_a), None)
            .await?;
        assert_eq!(resolved, actor_a);

        let existing = db.resolve_port_actor("telegram", &key, None, None).await?;
        assert_eq!(existing, actor_a);

        let overridden = db
            .resolve_port_actor("telegram", &key, Some(&actor_b), None)
            .await?;
        assert_eq!(overridden, actor_b);

        let existing_after_override = db.resolve_port_actor("telegram", &key, None, None).await?;
        assert_eq!(existing_after_override, actor_b);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_port_actor_uses_default_and_errors_without_actor() -> Result<()> {
        let path = tmp_db_path("default");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor = Uri::from_parts("devmode", "actor", Some("default-a"))?;
        let key = Uri::from_parts("borg", "conversation", Some("c2"))?;
        db.upsert_actor(&actor, "A", "prompt", "RUNNING").await?;

        let resolved = db
            .resolve_port_actor("http", &key, None, Some(&actor))
            .await?;
        assert_eq!(resolved, actor);

        let key_missing = Uri::from_parts("borg", "conversation", Some("c3"))?;
        let err = db
            .resolve_port_actor("http", &key_missing, None, None)
            .await
            .expect_err("expected missing actor binding error");
        assert!(err.to_string().contains("no actor binding"));
        Ok(())
    }
}
