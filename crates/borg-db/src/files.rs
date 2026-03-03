use anyhow::{Context, Result};
use borg_core::Uri;
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{BorgDb, FileRecord};

impl BorgDb {
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_file(
        &self,
        file_id: &Uri,
        backend: &str,
        storage_key: &str,
        content_type: &str,
        size_bytes: i64,
        sha512: &str,
        owner_uri: Option<&Uri>,
        metadata_json: &Value,
    ) -> Result<FileRecord> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO files(
                file_id,
                backend,
                storage_key,
                content_type,
                size_bytes,
                sha512,
                owner_uri,
                metadata_json,
                deleted_at,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10)
            ON CONFLICT(file_id) DO UPDATE SET
              backend = excluded.backend,
              storage_key = excluded.storage_key,
              content_type = excluded.content_type,
              size_bytes = excluded.size_bytes,
              sha512 = excluded.sha512,
              owner_uri = COALESCE(files.owner_uri, excluded.owner_uri),
              metadata_json = excluded.metadata_json,
              deleted_at = NULL,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(file_id.to_string())
        .bind(backend)
        .bind(storage_key)
        .bind(content_type)
        .bind(size_bytes)
        .bind(sha512)
        .bind(owner_uri.map(ToString::to_string))
        .bind(metadata_json.to_string())
        .bind(now.clone())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to upsert file")?;

        self.get_file(file_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("failed to reload upserted file {}", file_id))
    }

    pub async fn get_file(&self, file_id: &Uri) -> Result<Option<FileRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                file_id,
                backend,
                storage_key,
                content_type,
                size_bytes,
                sha512,
                owner_uri,
                metadata_json,
                deleted_at,
                created_at,
                updated_at
            FROM files
            WHERE file_id = ?1
            LIMIT 1
            "#,
        )
        .bind(file_id.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to get file")?;

        row.map(file_from_row).transpose()
    }

    pub async fn file_exists(&self, file_id: &Uri) -> Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT 1 as found
            FROM files
            WHERE file_id = ?1
              AND deleted_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(file_id.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to check file existence")?;
        Ok(row.is_some())
    }

    pub async fn soft_delete_file(&self, file_id: &Uri) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let affected = sqlx::query(
            r#"
            UPDATE files
            SET deleted_at = ?1, updated_at = ?2
            WHERE file_id = ?3
              AND deleted_at IS NULL
            "#,
        )
        .bind(now.clone())
        .bind(now)
        .bind(file_id.to_string())
        .execute(self.pool())
        .await
        .context("failed to soft delete file")?
        .rows_affected();
        Ok(affected)
    }
}

fn file_from_row(row: sqlx::sqlite::SqliteRow) -> Result<FileRecord> {
    let metadata_json_raw: String = row.try_get("metadata_json")?;
    let metadata_json =
        serde_json::from_str(&metadata_json_raw).context("invalid files.metadata_json value")?;
    let deleted_at_raw: Option<String> = row.try_get("deleted_at")?;
    Ok(FileRecord {
        file_id: Uri::parse(&row.try_get::<String, _>("file_id")?)?,
        backend: row.try_get("backend")?,
        storage_key: row.try_get("storage_key")?,
        content_type: row.try_get("content_type")?,
        size_bytes: row.try_get("size_bytes")?,
        sha512: row.try_get("sha512")?,
        owner_uri: row
            .try_get::<Option<String>, _>("owner_uri")?
            .map(|value| Uri::parse(&value))
            .transpose()?,
        metadata_json,
        deleted_at: deleted_at_raw.as_deref().map(parse_ts).transpose()?,
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
            "borg-db-files-{test_name}-{}.db",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[tokio::test]
    async fn files_upsert_roundtrip_and_soft_delete() -> Result<()> {
        let path = tmp_db_path("crud");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let file_id = Uri::from_parts(
            "borg",
            "audio",
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"),
        )?;
        let owner = Uri::from_parts("borg", "user", Some("alice"))?;
        let record = db
            .upsert_file(
                &file_id,
                "local",
                "audio/ab/cd/abcdef",
                "audio/mpeg",
                42,
                "abcdef0123456789",
                Some(&owner),
                &serde_json::json!({"session_id":"borg:session:s1"}),
            )
            .await?;

        assert_eq!(record.file_id, file_id);
        assert_eq!(record.owner_uri.as_ref(), Some(&owner));
        assert!(db.file_exists(&file_id).await?);

        let deleted = db.soft_delete_file(&file_id).await?;
        assert_eq!(deleted, 1);
        assert!(!db.file_exists(&file_id).await?);

        Ok(())
    }
}
