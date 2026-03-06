use anyhow::{Context, Result, anyhow};
use borg_core::Uri;
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::BorgDb;

impl BorgDb {
    pub async fn list_port_actor_bindings(
        &self,
        port: &str,
        limit: usize,
    ) -> Result<Vec<(Uri, Option<Uri>)>> {
        Ok(self
            .list_port_binding_records(port, limit)
            .await?
            .into_iter()
            .map(|(conversation_key, _session_id, actor_id)| (conversation_key, actor_id))
            .collect())
    }

    pub async fn upsert_port_actor_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
        actor_id: &Uri,
    ) -> Result<()> {
        if self.get_actor(actor_id).await?.is_none() {
            return Err(anyhow!("actor not found for actor_id {}", actor_id));
        }
        let session_id = self
            .get_port_binding_full_record(port, conversation_key)
            .await?
            .map(|(_conversation_key, session_id, _bound_actor_id)| session_id)
            .unwrap_or_else(|| conversation_key.clone());

        self.upsert_port_binding_full_record(
            port,
            conversation_key,
            &session_id,
            Some(Some(actor_id)),
        )
        .await
        .context("failed to upsert port actor binding")
    }

    pub async fn get_port_actor_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<Option<Uri>> {
        Ok(self
            .get_port_binding_full_record(port, conversation_key)
            .await?
            .and_then(|(_conversation_key, _session_id, actor_id)| actor_id))
    }

    pub async fn clear_port_actor_binding(
        &self,
        port: &str,
        conversation_key: &Uri,
    ) -> Result<u64> {
        let updated = sqlx::query(
            r#"
            UPDATE port_bindings
            SET actor_id = NULL,
                updated_at = ?3
            WHERE port = ?1 AND conversation_key = ?2
            "#,
        )
        .bind(port)
        .bind(conversation_key.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(self.pool())
        .await
        .context("failed to clear port actor binding")?
        .rows_affected();
        Ok(updated)
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

        let session_id = self
            .get_port_binding_full_record(port, conversation_key)
            .await?
            .map(|(_conversation_key, session_id, _bound_actor_id)| session_id)
            .unwrap_or_else(|| conversation_key.clone());
        let actor_id = deterministic_actor_id(port, conversation_key)?;
        if self.get_actor(&actor_id).await?.is_none() {
            self.upsert_actor(
                &actor_id,
                &fallback_actor_name(port, conversation_key),
                "",
                "RUNNING",
            )
            .await?;
        }
        self.upsert_port_binding_full_record(
            port,
            conversation_key,
            &session_id,
            Some(Some(&actor_id)),
        )
        .await?;
        Ok(actor_id)
    }
}

fn fallback_actor_name(port: &str, conversation_key: &Uri) -> String {
    let tail = conversation_key
        .as_str()
        .rsplit(':')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("conversation");
    format!("{port}-{tail}")
}

fn deterministic_actor_id(port: &str, conversation_key: &Uri) -> Result<Uri> {
    let mut hasher = Sha256::new();
    hasher.update(port.as_bytes());
    hasher.update(b":");
    hasher.update(conversation_key.as_str().as_bytes());
    let digest = hex::encode(hasher.finalize());
    Uri::from_parts("borg", "actor", Some(digest.as_str()))
        .context("failed to build deterministic actor uri")
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
    async fn resolve_port_actor_uses_default_and_autocreates_without_actor() -> Result<()> {
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
        let created = db
            .resolve_port_actor("http", &key_missing, None, None)
            .await?;
        assert_eq!(created, deterministic_actor_id("http", &key_missing)?);
        assert!(db.get_actor(&created).await?.is_some());

        let created_again = db
            .resolve_port_actor("http", &key_missing, None, None)
            .await?;
        assert_eq!(created_again, created);
        Ok(())
    }

    #[tokio::test]
    async fn clear_port_actor_binding_nulls_actor_id() -> Result<()> {
        let path = tmp_db_path("clear-binding");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor = Uri::from_parts("devmode", "actor", Some("clear-a"))?;
        let key = Uri::from_parts("borg", "conversation", Some("clear-c"))?;
        db.upsert_actor(&actor, "A", "prompt", "RUNNING").await?;
        db.upsert_port_actor_binding("telegram", &key, &actor)
            .await?;
        assert_eq!(
            db.get_port_actor_binding("telegram", &key).await?,
            Some(actor.clone())
        );

        let updated = db.clear_port_actor_binding("telegram", &key).await?;
        assert_eq!(updated, 1);
        assert_eq!(db.get_port_actor_binding("telegram", &key).await?, None);
        Ok(())
    }

    #[tokio::test]
    async fn list_port_actor_bindings_returns_rows_for_port() -> Result<()> {
        let path = tmp_db_path("list-bindings");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let actor = Uri::from_parts("devmode", "actor", Some("list-a"))?;
        let key = Uri::from_parts("borg", "conversation", Some("list-c"))?;
        db.upsert_actor(&actor, "A", "prompt", "RUNNING").await?;
        db.upsert_port_actor_binding("telegram", &key, &actor)
            .await?;

        let rows = db.list_port_actor_bindings("telegram", 10).await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, key);
        assert_eq!(rows[0].1, Some(actor));
        Ok(())
    }
}
