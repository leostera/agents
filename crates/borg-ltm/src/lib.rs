use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow};
use borg_core::Entity;
use chrono::{DateTime, Utc};
use indradb::{
    AllVertexQuery, Database, Edge, Identifier, QueryExt, RocksdbDatastore, SpecificVertexQuery,
    Vertex, util,
};
use serde_json::Value;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Clone)]
pub struct MemoryStore {
    db: Arc<Mutex<Database<RocksdbDatastore>>>,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed creating ltm path {}", path.display()))?;
        let db = RocksdbDatastore::new_db(path)
            .with_context(|| format!("failed to open indradb rocksdb at {}", path.display()))?;

        info!(target: "borg_ltm", path = %path.display(), "initialized indradb-backed memory store");
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        info!(target: "borg_ltm", "running memory migrations (indradb indexes)");
        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;

        db.index_property(id("entity_id")?)?;
        db.index_property(id("natural_key")?)?;
        db.index_property(id("entity_type")?)?;
        db.index_property(id("label")?)?;
        db.index_property(id("search_text")?)?;

        info!(target: "borg_ltm", "memory migrations completed");
        Ok(())
    }

    pub async fn upsert_entity(
        &self,
        entity_type: &str,
        label: &str,
        props: &Value,
        natural_key: Option<&str>,
    ) -> Result<String> {
        let now = Utc::now().to_rfc3339();
        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;

        let existing = if let Some(nk) = natural_key {
            self.find_vertex_by_property(&db, "natural_key", &Value::String(nk.to_string()))?
        } else {
            None
        };

        let vertex_id = existing.unwrap_or_else(Uuid::new_v4);
        let vertex = Vertex::with_id(vertex_id, id(&sanitize_identifier(entity_type))?);
        let _ = db.create_vertex(&vertex)?;

        let props_json = props.to_string();
        let search_text = format!("{} {} {}", label, entity_type, props_json);

        set_vertex_prop_string(&db, vertex_id, "entity_id", &vertex_id.to_string())?;
        set_vertex_prop_string(&db, vertex_id, "entity_type", entity_type)?;
        set_vertex_prop_string(&db, vertex_id, "label", label)?;
        set_vertex_prop_string(&db, vertex_id, "props_json", &props_json)?;
        set_vertex_prop_string(&db, vertex_id, "updated_at", &now)?;
        set_vertex_prop_string(&db, vertex_id, "search_text", &search_text)?;

        let created = self
            .get_string_prop(&db, vertex_id, "created_at")?
            .unwrap_or_else(|| now.clone());
        set_vertex_prop_string(&db, vertex_id, "created_at", &created)?;

        if let Some(nk) = natural_key {
            set_vertex_prop_string(&db, vertex_id, "natural_key", nk)?;
        }

        let entity_id = vertex_id.to_string();
        debug!(target: "borg_ltm", entity_id, label, entity_type, "entity upsert committed");
        Ok(entity_id)
    }

    pub async fn link(
        &self,
        from_entity_id: &str,
        rel_type: &str,
        to_entity_id: &str,
        props: &Value,
    ) -> Result<String> {
        let from = Uuid::parse_str(from_entity_id).context("invalid from entity id")?;
        let to = Uuid::parse_str(to_entity_id).context("invalid to entity id")?;

        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;
        let rel = sanitize_identifier(rel_type);
        let edge = Edge::new(from, id(&rel)?, to);
        let created = db.create_edge(&edge)?;

        let rel_id = format!("{}:{}:{}", from, rel, to);
        let now = Utc::now().to_rfc3339();
        if created {
            set_edge_prop_string(&db, &edge, "created_at", &now)?;
            set_edge_prop_string(&db, &edge, "props_json", &props.to_string())?;
        }

        info!(target: "borg_ltm", rel_id, rel_type, from = from_entity_id, to = to_entity_id, "relation linked");
        Ok(rel_id)
    }

    pub async fn get_entity(&self, entity_id: &str) -> Result<Option<Entity>> {
        let vertex_id = match Uuid::parse_str(entity_id) {
            Ok(id) => id,
            Err(_) => return Ok(None),
        };

        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;
        self.fetch_entity_by_vertex_id(&db, vertex_id)
    }

    pub async fn search(
        &self,
        text: &str,
        entity_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Entity>> {
        let query = text.trim().to_lowercase();
        let limit = limit.max(1);
        debug!(target: "borg_ltm", query, ?entity_type, limit, "running memory search");

        if query.is_empty() {
            return Ok(Vec::new());
        }

        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;
        let vertices = util::extract_vertices(db.get(AllVertexQuery)?).unwrap_or_default();

        let mut out = Vec::new();
        for vertex in vertices {
            let Some(entity) = self.fetch_entity_by_vertex_id(&db, vertex.id)? else {
                continue;
            };

            if let Some(expected_type) = entity_type {
                if entity.entity_type != expected_type {
                    continue;
                }
            }

            let haystack = format!(
                "{} {}",
                entity.label.to_lowercase(),
                entity.props.to_string().to_lowercase()
            );
            if haystack.contains(&query) {
                out.push(entity);
                if out.len() >= limit {
                    break;
                }
            }
        }

        Ok(out)
    }

    fn fetch_entity_by_vertex_id(
        &self,
        db: &Database<RocksdbDatastore>,
        vertex_id: Uuid,
    ) -> Result<Option<Entity>> {
        let vertex = util::extract_vertices(db.get(SpecificVertexQuery::single(vertex_id))?)
            .and_then(|mut v| v.pop());

        let Some(vertex) = vertex else {
            return Ok(None);
        };

        let props = self.vertex_props_map(db, vertex.id)?;
        let Some(entity_id) = props
            .get("entity_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
        else {
            return Ok(None);
        };

        let entity_type = props
            .get("entity_type")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| vertex.t.to_string());
        let label = props
            .get("label")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| entity_type.clone());
        let props_json = props
            .get("props_json")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let created_at = props
            .get("created_at")
            .and_then(|v| v.as_str())
            .map(parse_ts)
            .transpose()?
            .unwrap_or_else(Utc::now);
        let updated_at = props
            .get("updated_at")
            .and_then(|v| v.as_str())
            .map(parse_ts)
            .transpose()?
            .unwrap_or_else(Utc::now);

        Ok(Some(Entity {
            entity_id,
            entity_type,
            label,
            props: serde_json::from_str(props_json).unwrap_or(Value::Null),
            created_at,
            updated_at,
        }))
    }

    fn get_string_prop(
        &self,
        db: &Database<RocksdbDatastore>,
        vertex_id: Uuid,
        key: &str,
    ) -> Result<Option<String>> {
        let props = self.vertex_props_map(db, vertex_id)?;
        Ok(props
            .get(key)
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned))
    }

    fn vertex_props_map(
        &self,
        db: &Database<RocksdbDatastore>,
        vertex_id: Uuid,
    ) -> Result<HashMap<String, Value>> {
        let mut map = HashMap::new();
        let q = SpecificVertexQuery::single(vertex_id).properties()?;
        let out = db.get(q)?;
        let props = util::extract_vertex_properties(out).unwrap_or_default();

        for vertex_props in props {
            for prop in vertex_props.props {
                map.insert(prop.name.to_string(), (*prop.value).clone());
            }
        }
        Ok(map)
    }

    fn find_vertex_by_property(
        &self,
        db: &Database<RocksdbDatastore>,
        prop_name: &str,
        expected_value: &Value,
    ) -> Result<Option<Uuid>> {
        let vertices = util::extract_vertices(db.get(AllVertexQuery)?).unwrap_or_default();
        for v in vertices {
            let props = self.vertex_props_map(db, v.id)?;
            if props.get(prop_name) == Some(expected_value) {
                return Ok(Some(v.id));
            }
        }
        Ok(None)
    }
}

fn id(value: &str) -> Result<Identifier> {
    Identifier::new(value).map_err(|_| anyhow!("invalid indradb identifier: {}", value))
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

fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(ts)
        .map_err(|_| anyhow!("invalid RFC3339 timestamp"))?
        .with_timezone(&Utc))
}

fn set_vertex_prop_string(
    db: &Database<RocksdbDatastore>,
    vertex_id: Uuid,
    key: &str,
    value: &str,
) -> Result<()> {
    let json: indradb::Json = Value::String(value.to_string()).into();
    db.set_properties(SpecificVertexQuery::single(vertex_id), id(key)?, &json)?;
    Ok(())
}

fn set_edge_prop_string(
    db: &Database<RocksdbDatastore>,
    edge: &Edge,
    key: &str,
    value: &str,
) -> Result<()> {
    let json: indradb::Json = Value::String(value.to_string()).into();
    db.set_properties(
        indradb::SpecificEdgeQuery::single(edge.clone()),
        id(key)?,
        &json,
    )?;
    Ok(())
}
