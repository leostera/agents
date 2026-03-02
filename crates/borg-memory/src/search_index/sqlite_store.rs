use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result;
use borg_core::Entity;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tracing::info;

use crate::SearchQuery;

const MEMORY_DB_FILE: &str = "memory.db";

#[derive(Clone)]
pub(crate) struct SqliteSearchIndex {
    db_path: PathBuf,
    schema_ready: Arc<AtomicBool>,
    schema_lock: Arc<tokio::sync::Mutex<()>>,
}

impl SqliteSearchIndex {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = resolve_db_path(path.as_ref());
        let parent = db_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid sqlite db path"))?;
        std::fs::create_dir_all(parent)?;
        Ok(Self {
            db_path,
            schema_ready: Arc::new(AtomicBool::new(false)),
            schema_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        self.ensure_migrated().await?;
        info!(
            target: "borg_memory",
            path = %self.db_path.display(),
            "sqlite fts search index ready"
        );
        Ok(())
    }

    pub(crate) async fn upsert_entity(&self, entity: &Entity) -> Result<()> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let namespace = entity
            .props
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let kind = entity
            .props
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or(entity.entity_type.as_str());
        let text = format!("{} {}", entity.label, entity.props);

        sqlx::query("DELETE FROM search_fts WHERE entity_id = ?1")
            .bind(entity.entity_id.as_str())
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO search_fts(entity_id, namespace, kind, label, text) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(entity.entity_id.as_str())
        .bind(namespace)
        .bind(kind)
        .bind(&entity.label)
        .bind(text)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn search(&self, query: &SearchQuery, limit: usize) -> Result<Vec<String>> {
        self.ensure_migrated().await?;
        let Some(query_text) = query.text() else {
            return Ok(Vec::new());
        };

        let pool = self.open_pool().await?;
        let limit = i64::try_from(limit.max(1)).unwrap_or(25);
        let mut out = Vec::new();

        let match_rows = sqlx::query(
            r#"
            SELECT entity_id
            FROM search_fts
            WHERE search_fts MATCH ?1
              AND (?2 IS NULL OR namespace = ?2)
              AND (?3 IS NULL OR kind = ?3)
            ORDER BY bm25(search_fts)
            LIMIT ?4
            "#,
        )
        .bind(query_text)
        .bind(query.ns.as_deref())
        .bind(query.kind.as_deref())
        .bind(limit)
        .fetch_all(&pool)
        .await;

        match match_rows {
            Ok(rows) => {
                for row in rows {
                    let entity_id: String = row.try_get(0)?;
                    out.push(entity_id);
                }
                Ok(out)
            }
            Err(_) => {
                let fallback = format!("%{}%", query_text.to_lowercase());
                let rows = sqlx::query(
                    r#"
                    SELECT entity_id
                    FROM search_fts
                    WHERE (lower(label) LIKE ?1 OR lower(text) LIKE ?1)
                      AND (?2 IS NULL OR namespace = ?2)
                      AND (?3 IS NULL OR kind = ?3)
                    LIMIT ?4
                    "#,
                )
                .bind(fallback)
                .bind(query.ns.as_deref())
                .bind(query.kind.as_deref())
                .bind(limit)
                .fetch_all(&pool)
                .await?;
                for row in rows {
                    let entity_id: String = row.try_get(0)?;
                    out.push(entity_id);
                }
                Ok(out)
            }
        }
    }

    async fn ensure_migrated(&self) -> Result<()> {
        if self.schema_ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = self.schema_lock.lock().await;
        if self.schema_ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        let pool = self.open_pool().await?;
        sqlx::raw_sql(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
                entity_id UNINDEXED,
                namespace UNINDEXED,
                kind UNINDEXED,
                label,
                text
            );
            "#,
        )
        .execute(&pool)
        .await?;
        self.schema_ready.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn open_pool(&self) -> Result<SqlitePool> {
        let options = SqliteConnectOptions::new()
            .filename(&self.db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePool::connect_with(options).await?;
        sqlx::query("PRAGMA busy_timeout = 5000")
            .execute(&pool)
            .await?;
        Ok(pool)
    }
}

fn resolve_db_path(path: &Path) -> PathBuf {
    if path.extension().and_then(|ext| ext.to_str()) == Some("db") {
        path.to_path_buf()
    } else {
        path.join(MEMORY_DB_FILE)
    }
}
