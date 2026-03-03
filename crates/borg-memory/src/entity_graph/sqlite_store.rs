use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow};
use borg_core::{Entity, EntityPropValue, EntityProps, Uri};
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tracing::{debug, info};
use url::Url;
use uuid::Uuid;

use crate::fact_store::{FactArity, FactRecord, FactValue};

const MEMORY_DB_FILE: &str = "memory.db";

#[derive(Clone)]
pub(crate) struct SqliteEntityGraph {
    db_path: PathBuf,
    schema_ready: Arc<AtomicBool>,
    schema_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, Clone)]
pub(crate) struct TraversalEdge {
    pub(crate) source_entity_id: String,
    pub(crate) target_entity_id: String,
    pub(crate) relation: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TraversalResult {
    pub(crate) entities: Vec<Entity>,
    pub(crate) edges: Vec<TraversalEdge>,
}

impl SqliteEntityGraph {
    pub(crate) fn new(path: &Path) -> Result<Self> {
        let db_path = resolve_db_path(path);
        let parent = db_path
            .parent()
            .ok_or_else(|| anyhow!("invalid sqlite db path"))?;
        std::fs::create_dir_all(parent)?;
        Ok(Self {
            db_path,
            schema_ready: Arc::new(AtomicBool::new(false)),
            schema_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        self.ensure_migrated().await
    }

    pub(crate) async fn upsert_entity(
        &self,
        entity_type: &str,
        label: &str,
        props: &EntityProps,
        natural_key: Option<&str>,
    ) -> Result<String> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let now = Utc::now().to_rfc3339();

        let existing = if let Some(natural_key) = natural_key {
            sqlx::query("SELECT entity_id, created_at FROM entities WHERE natural_key = ?1")
                .bind(natural_key)
                .fetch_optional(&pool)
                .await?
        } else {
            None
        };

        let (entity_id, created_at) = if let Some(row) = existing {
            (row.try_get::<String, _>(0)?, row.try_get::<String, _>(1)?)
        } else {
            let entity_id = if let Some(natural_key) = natural_key {
                if is_uri_like(natural_key) {
                    natural_key.to_string()
                } else {
                    new_entity_id(entity_type)
                }
            } else {
                new_entity_id(entity_type)
            };
            (entity_id, now.clone())
        };

        sqlx::query(
            r#"
            INSERT INTO entities(entity_id, entity_type, label, props_json, natural_key, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(entity_id) DO UPDATE SET
                entity_type = excluded.entity_type,
                label = excluded.label,
                props_json = excluded.props_json,
                natural_key = excluded.natural_key,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&entity_id)
        .bind(entity_type)
        .bind(label)
        .bind(serde_json::to_string(props)?)
        .bind(natural_key)
        .bind(created_at)
        .bind(now.clone())
        .execute(&pool)
        .await?;

        self.upsert_schema_projection(&pool, &entity_id, label, props, &now)
            .await?;

        debug!(
            target: "borg_memory",
            entity_id,
            label,
            entity_type,
            "entity upsert committed"
        );
        Ok(entity_id)
    }

    pub(crate) async fn link(
        &self,
        from_entity_id: &str,
        rel_type: &str,
        to_entity_id: &str,
        props: &EntityProps,
    ) -> Result<String> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        ensure_entity_exists(&pool, from_entity_id).await?;
        ensure_entity_exists(&pool, to_entity_id).await?;

        let rel = sanitize_identifier(rel_type);
        let rel_id = format!("{}:{}:{}", from_entity_id, rel, to_entity_id);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO entity_edges(edge_id, source_entity_id, relation, target_entity_id, props_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(edge_id) DO UPDATE SET
                props_json = excluded.props_json
            "#,
        )
        .bind(&rel_id)
        .bind(from_entity_id)
        .bind(&rel)
        .bind(to_entity_id)
        .bind(serde_json::to_string(props)?)
        .bind(now)
        .execute(&pool)
        .await?;

        info!(
            target: "borg_memory",
            rel_id,
            rel_type,
            from = from_entity_id,
            to = to_entity_id,
            "relation linked"
        );
        Ok(rel_id)
    }

    pub(crate) async fn get_entity(&self, entity_id: &str) -> Result<Option<Entity>> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let row = sqlx::query(
            "SELECT entity_id, entity_type, label, props_json, created_at, updated_at FROM entities WHERE entity_id = ?1",
        )
        .bind(entity_id)
        .fetch_optional(&pool)
        .await?;
        row.map(row_to_entity).transpose()
    }

    pub(crate) async fn is_empty(&self) -> Result<bool> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let row = sqlx::query("SELECT COUNT(*) FROM entities")
            .fetch_one(&pool)
            .await?;
        let count: i64 = row.try_get(0)?;
        Ok(count == 0)
    }

    pub(crate) async fn search(
        &self,
        text: &str,
        entity_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Entity>> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let query = text.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{}%", query);
        let limit = i64::try_from(limit.max(1)).unwrap_or(25);

        let rows = sqlx::query(
            r#"
            SELECT entity_id, entity_type, label, props_json, created_at, updated_at
            FROM entities
            WHERE (?1 IS NULL OR entity_type = ?1)
              AND (lower(label) LIKE ?2 OR lower(props_json) LIKE ?2)
            ORDER BY updated_at DESC
            LIMIT ?3
            "#,
        )
        .bind(entity_type)
        .bind(like)
        .bind(limit)
        .fetch_all(&pool)
        .await?;

        rows.into_iter().map(row_to_entity).collect()
    }

    pub(crate) async fn expand_subgraph(
        &self,
        seed_entity_ids: &[String],
        max_vertices: usize,
    ) -> Result<TraversalResult> {
        self.ensure_migrated().await?;
        let pool = self.open_pool().await?;
        let max_vertices = max_vertices.max(1);

        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();
        for seed in seed_entity_ids {
            if visited.insert(seed.clone()) {
                queue.push_back(seed.clone());
            }
            if visited.len() >= max_vertices {
                break;
            }
        }

        let mut edge_set: HashSet<(String, String, String)> = HashSet::new();
        while let Some(current) = queue.pop_front() {
            let rows = sqlx::query(
                r#"
                SELECT source_entity_id, relation, target_entity_id
                FROM entity_edges
                WHERE source_entity_id = ?1 OR target_entity_id = ?1
                "#,
            )
            .bind(&current)
            .fetch_all(&pool)
            .await?;

            for row in rows {
                let source: String = row.try_get(0)?;
                let relation: String = row.try_get(1)?;
                let target: String = row.try_get(2)?;
                if edge_set.insert((source.clone(), relation.clone(), target.clone())) {
                    for neighbor in [source.clone(), target.clone()] {
                        if visited.len() >= max_vertices {
                            break;
                        }
                        if visited.insert(neighbor.clone()) {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        let mut entities = Vec::new();
        let mut edge_entities = Vec::new();
        for (source, relation, target) in edge_set {
            if visited.contains(&source) && visited.contains(&target) {
                edge_entities.push(TraversalEdge {
                    source_entity_id: source,
                    target_entity_id: target,
                    relation,
                });
            }
        }

        for entity_id in visited {
            if let Some(entity) = self.get_entity(&entity_id).await? {
                entities.push(entity);
            }
        }

        Ok(TraversalResult {
            entities,
            edges: edge_entities,
        })
    }

    pub(crate) async fn apply_fact(&self, fact: &FactRecord) -> Result<()> {
        let (namespace, kind, _) = split_entity_uri(fact.entity.as_str())?;
        let entity_type = format!("{}:kind:{}", namespace, kind);
        let natural_key = fact.entity.to_string();
        let field_key = field_uri_to_prop_key(fact.field.as_str());
        let field_value = fact_value_to_prop(&fact.value)?;

        let existing = self.get_entity(fact.entity.as_str()).await?;
        let mut props = existing
            .as_ref()
            .map(|entity| entity.props.clone())
            .unwrap_or_default();

        props.insert("uri".to_string(), EntityPropValue::Text(fact.entity.to_string()));
        props.insert(
            "namespace".to_string(),
            EntityPropValue::Text(namespace.clone()),
        );
        props.insert("kind".to_string(), EntityPropValue::Text(kind.clone()));
        props.insert(
            "last_tx".to_string(),
            EntityPropValue::Text(fact.tx_id.to_string()),
        );
        props.insert(
            "last_stated_at".to_string(),
            EntityPropValue::Text(fact.stated_at.to_rfc3339()),
        );
        self.upsert_field_value(&mut props, &field_key, field_value, fact.arity);

        let mut label = existing
            .as_ref()
            .map(|entity| entity.label.clone())
            .unwrap_or_else(|| fact.entity.to_string());
        if fact.field.as_str() == "borg:fields:name"
            && let FactValue::Text(name) = &fact.value
        {
            label = name.clone();
        }

        self.upsert_entity(&entity_type, &label, &props, Some(&natural_key))
            .await?;
        Ok(())
    }

    fn upsert_field_value(
        &self,
        object: &mut EntityProps,
        field_key: &str,
        field_value: EntityPropValue,
        arity: FactArity,
    ) {
        match arity {
            FactArity::One => {
                object.insert(field_key.to_string(), field_value);
            }
            FactArity::Many => {
                let Some(existing) = object.get_mut(field_key) else {
                    object.insert(
                        field_key.to_string(),
                        EntityPropValue::List(vec![field_value]),
                    );
                    return;
                };

                match existing {
                    EntityPropValue::List(values) => {
                        if !values.contains(&field_value) {
                            values.push(field_value);
                        }
                    }
                    prior => {
                        if *prior == field_value {
                            *prior = EntityPropValue::List(vec![field_value]);
                        } else {
                            let previous = prior.clone();
                            *prior = EntityPropValue::List(vec![previous, field_value]);
                        }
                    }
                }
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
            CREATE TABLE IF NOT EXISTS entities (
                entity_id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                label TEXT NOT NULL,
                props_json TEXT NOT NULL,
                natural_key TEXT UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
            CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at);

            CREATE TABLE IF NOT EXISTS entity_edges (
                edge_id TEXT PRIMARY KEY,
                source_entity_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                target_entity_id TEXT NOT NULL,
                props_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON entity_edges(source_entity_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON entity_edges(target_entity_id);
            CREATE INDEX IF NOT EXISTS idx_edges_relation ON entity_edges(relation);

            CREATE TABLE IF NOT EXISTS schemas (
                schema_uri TEXT PRIMARY KEY,
                schema_kind TEXT NOT NULL,
                label TEXT NOT NULL,
                props_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_schemas_kind ON schemas(schema_kind);
            "#,
        )
        .execute(&pool)
        .await?;
        self.schema_ready.store(true, Ordering::SeqCst);

        info!(
            target: "borg_memory",
            path = %self.db_path.display(),
            "sqlite entity graph migrations completed"
        );
        Ok(())
    }

    async fn upsert_schema_projection(
        &self,
        pool: &SqlitePool,
        entity_id: &str,
        label: &str,
        props: &EntityProps,
        updated_at: &str,
    ) -> Result<()> {
        let Ok((namespace, kind, _)) = split_entity_uri(entity_id) else {
            return Ok(());
        };
        if namespace != "borg" {
            return Ok(());
        }

        let schema_kind = match kind.as_str() {
            "kind" | "field" | "namespace" | "schema" => kind,
            _ => return Ok(()),
        };

        sqlx::query(
            r#"
            INSERT INTO schemas(schema_uri, schema_kind, label, props_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(schema_uri) DO UPDATE SET
                schema_kind = excluded.schema_kind,
                label = excluded.label,
                props_json = excluded.props_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(entity_id)
        .bind(schema_kind)
        .bind(label)
        .bind(serde_json::to_string(props)?)
        .bind(updated_at)
        .execute(pool)
        .await?;
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

async fn ensure_entity_exists(pool: &SqlitePool, entity_id: &str) -> Result<()> {
    let exists = sqlx::query("SELECT 1 FROM entities WHERE entity_id = ?1")
        .bind(entity_id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(anyhow!("invalid entity id"))
    }
}

fn row_to_entity(row: sqlx::sqlite::SqliteRow) -> Result<Entity> {
    let entity_id: String = row.try_get(0)?;
    let entity_type: String = row.try_get(1)?;
    let label: String = row.try_get(2)?;
    let props_json: String = row.try_get(3)?;
    let created_at: String = row.try_get(4)?;
    let updated_at: String = row.try_get(5)?;

    Ok(Entity {
        entity_id: Uri::parse(&entity_id)?,
        entity_type: Uri::parse(&entity_type)?,
        label,
        props: serde_json::from_str(&props_json)?,
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
    })
}

fn split_entity_uri(uri: &str) -> Result<(String, String, String)> {
    let parsed = Url::parse(uri)?;
    let ns = parsed.scheme().to_string();
    let opaque = parsed.path().trim_start_matches('/');
    let mut parts = opaque.split(':');
    let kind = parts
        .next()
        .ok_or_else(|| anyhow!("invalid entity uri: {}", uri))?
        .to_string();
    let id = parts
        .next()
        .ok_or_else(|| anyhow!("invalid entity uri: {}", uri))?
        .to_string();
    if parts.next().is_some() {
        return Err(anyhow!("invalid entity uri: {}", uri));
    }
    if ns.is_empty() || kind.is_empty() || id.is_empty() {
        return Err(anyhow!("invalid entity uri: {}", uri));
    }
    Ok((ns, kind, id))
}

fn field_uri_to_prop_key(field_uri: &str) -> String {
    let candidate = field_uri
        .rsplit(':')
        .next()
        .unwrap_or(field_uri)
        .split('/')
        .next()
        .unwrap_or(field_uri);
    sanitize_identifier(candidate).to_lowercase()
}

fn fact_value_to_prop(value: &FactValue) -> Result<EntityPropValue> {
    Ok(match value {
        FactValue::Text(v) => EntityPropValue::Text(v.clone()),
        FactValue::Integer(v) => EntityPropValue::Integer(*v),
        FactValue::Float(v) => EntityPropValue::Float(*v),
        FactValue::Boolean(v) => EntityPropValue::Boolean(*v),
        FactValue::Bytes(v) => EntityPropValue::Bytes(v.clone()),
        FactValue::Ref(uri) => EntityPropValue::Ref(Uri::parse(uri.as_str())?),
        FactValue::Json(v) => EntityPropValue::Text(serde_json::to_string(v)?),
    })
}

fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.chars().take(255).collect()
    }
}

fn new_entity_id(entity_type: &str) -> String {
    let kind = sanitize_identifier(entity_type).to_lowercase();
    format!("borg:{}:{}", kind, Uuid::now_v7())
}

fn is_uri_like(value: &str) -> bool {
    let Ok(parsed) = Url::parse(value) else {
        return false;
    };
    let ns = parsed.scheme();
    let opaque = parsed.path().trim_start_matches('/');
    let mut parts = opaque.split(':');
    let Some(kind) = parts.next() else {
        return false;
    };
    let Some(id) = parts.next() else {
        return false;
    };
    parts.next().is_none() && !ns.is_empty() && !kind.is_empty() && !id.is_empty()
}

fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(ts)?.with_timezone(&Utc))
}

fn resolve_db_path(path: &Path) -> PathBuf {
    if path.extension().and_then(|ext| ext.to_str()) == Some("db") {
        path.to_path_buf()
    } else {
        path.join(MEMORY_DB_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::new_entity_id;

    #[test]
    fn new_entity_id_uses_borg_uri_shape() {
        let entity_id = new_entity_id("Movie");
        assert!(entity_id.starts_with("borg:movie:"));
    }
}
