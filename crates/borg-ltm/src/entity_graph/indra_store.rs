use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};
use borg_core::{Entity, Uri};
use chrono::{DateTime, Utc};
use indradb::{
    AllVertexQuery, Database, Edge, Identifier, QueryExt, RocksdbDatastore, SpecificEdgeQuery,
    SpecificVertexQuery, Vertex, util,
};
use serde_json::Value;
use tracing::{debug, info};
use url::Url;
use uuid::Uuid;

use crate::fact_store::{FactRecord, FactValue};

const ENTITY_GRAPH_DIR: &str = "entity_graph";
const ENTITY_ID_NAMESPACE: &str = "borg";

#[derive(Clone)]
pub(crate) struct IndraEntityGraph {
    db: Arc<Mutex<Database<RocksdbDatastore>>>,
}

impl IndraEntityGraph {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        let graph_path = root.join(ENTITY_GRAPH_DIR);
        std::fs::create_dir_all(&graph_path)?;
        let db = RocksdbDatastore::new_db(&graph_path)?;

        info!(
            target: "borg_ltm",
            path = %graph_path.display(),
            "initialized indradb-backed entity graph"
        );

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        info!(target: "borg_ltm", "running entity-graph migrations");
        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;

        db.index_property(id("entity_id")?)?;
        db.index_property(id("natural_key")?)?;
        db.index_property(id("entity_type")?)?;
        db.index_property(id("label")?)?;
        db.index_property(id("search_text")?)?;

        info!(target: "borg_ltm", "entity-graph migrations completed");
        Ok(())
    }

    pub(crate) async fn upsert_entity(
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

        let entity_id = if let Some(natural_key) = natural_key {
            if is_uri_like(natural_key) {
                natural_key.to_string()
            } else {
                match self.get_string_prop(&db, vertex_id, "entity_id")? {
                    Some(existing_id) if is_valid_entity_id(&existing_id) => existing_id,
                    _ => new_entity_id(entity_type),
                }
            }
        } else {
            match self.get_string_prop(&db, vertex_id, "entity_id")? {
                Some(existing_id) if is_valid_entity_id(&existing_id) => existing_id,
                _ => new_entity_id(entity_type),
            }
        };

        set_vertex_prop_string(&db, vertex_id, "entity_id", &entity_id)?;
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

        debug!(target: "borg_ltm", entity_id, label, entity_type, "entity upsert committed");
        Ok(entity_id)
    }

    pub(crate) async fn link(
        &self,
        from_entity_id: &str,
        rel_type: &str,
        to_entity_id: &str,
        props: &Value,
    ) -> Result<String> {
        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;
        let from = self
            .find_vertex_by_property(&db, "entity_id", &Value::String(from_entity_id.to_string()))?
            .ok_or_else(|| anyhow!("invalid from entity id"))?;
        let to = self
            .find_vertex_by_property(&db, "entity_id", &Value::String(to_entity_id.to_string()))?
            .ok_or_else(|| anyhow!("invalid to entity id"))?;

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

    pub(crate) async fn get_entity(&self, entity_id: &str) -> Result<Option<Entity>> {
        let db = self.db.lock().map_err(|_| anyhow!("ltm lock poisoned"))?;
        let Some(vertex_id) =
            self.find_vertex_by_property(&db, "entity_id", &Value::String(entity_id.to_string()))?
        else {
            return Ok(None);
        };

        self.fetch_entity_by_vertex_id(&db, vertex_id)
    }

    pub(crate) async fn search(
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
                if entity.entity_type.as_str() != expected_type {
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

    pub(crate) async fn apply_fact(&self, fact: &FactRecord) -> Result<()> {
        let (namespace, kind, _) = split_entity_uri(fact.entity.as_str())?;
        let entity_type = format!("{}:kind:{}", namespace, kind);
        let natural_key = fact.entity.to_string();
        let field_key = field_uri_to_prop_key(fact.field.as_str());
        let field_value = fact_value_to_json(&fact.value);

        let existing = self.get_entity(fact.entity.as_str()).await?;
        let mut props = existing
            .as_ref()
            .map(|entity| entity.props.clone())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

        if !props.is_object() {
            props = Value::Object(serde_json::Map::new());
        }
        let obj = props
            .as_object_mut()
            .ok_or_else(|| anyhow!("props not object"))?;
        obj.insert("uri".to_string(), Value::String(fact.entity.to_string()));
        obj.insert("namespace".to_string(), Value::String(namespace.clone()));
        obj.insert("kind".to_string(), Value::String(kind.clone()));
        obj.insert("last_tx".to_string(), Value::String(fact.tx_id.to_string()));
        obj.insert(
            "last_stated_at".to_string(),
            Value::String(fact.stated_at.to_rfc3339()),
        );
        obj.insert(field_key, field_value);

        let mut label = existing
            .as_ref()
            .map(|entity| entity.label.clone())
            .unwrap_or_else(|| fact.entity.to_string());
        if fact.field.as_str() == "borg:fields:name" {
            if let FactValue::Text(name) = &fact.value {
                label = name.clone();
            }
        }

        self.upsert_entity(&entity_type, &label, &props, Some(&natural_key))
            .await?;
        Ok(())
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
            .unwrap_or_else(|| {
                format!(
                    "borg:kind:{}",
                    sanitize_identifier(&format!("{:?}", vertex.t)).to_lowercase()
                )
            });
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
            entity_id: Uri::parse(&entity_id)?,
            entity_type: Uri::parse(&entity_type)?,
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
        let out = match db.get(q) {
            Ok(out) => out,
            Err(err) => {
                if err.to_string().contains("entity not found") {
                    return Ok(map);
                }
                return Err(err.into());
            }
        };
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
        let vertices = match db.get(AllVertexQuery) {
            Ok(out) => util::extract_vertices(out).unwrap_or_default(),
            Err(err) => {
                if err.to_string().contains("entity not found") {
                    return Ok(None);
                }
                return Err(err.into());
            }
        };
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

fn fact_value_to_json(value: &FactValue) -> Value {
    match value {
        FactValue::Text(v) => Value::String(v.clone()),
        FactValue::Integer(v) => Value::Number((*v).into()),
        FactValue::Float(v) => serde_json::Number::from_f64(*v)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FactValue::Boolean(v) => Value::Bool(*v),
        FactValue::Bytes(v) => Value::Array(v.iter().map(|b| Value::Number((*b).into())).collect()),
        FactValue::Ref(uri) => Value::String(uri.to_string()),
    }
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
    format!("{}:{}:{}", ENTITY_ID_NAMESPACE, kind, Uuid::now_v7())
}

fn is_valid_entity_id(entity_id: &str) -> bool {
    let mut parts = entity_id.split(':');
    let Some(namespace) = parts.next() else {
        return false;
    };
    let Some(kind) = parts.next() else {
        return false;
    };
    let Some(id) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    if namespace.is_empty() || kind.is_empty() {
        return false;
    }
    let Ok(uuid) = Uuid::parse_str(id) else {
        return false;
    };
    ((uuid.as_u128() >> 76) & 0xF) == 0x7
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
    db.set_properties(SpecificEdgeQuery::single(edge.clone()), id(key)?, &json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_valid_entity_id, new_entity_id};
    use uuid::Uuid;

    #[test]
    fn new_entity_id_uses_borg_uri_shape() {
        let entity_id = new_entity_id("Movie");
        assert!(entity_id.starts_with("borg:movie:"));
        assert!(is_valid_entity_id(&entity_id));
    }

    #[test]
    fn valid_entity_id_requires_uuid_v7() {
        let legacy = format!("borg:movie:{}", Uuid::new_v4());
        assert!(!is_valid_entity_id(&legacy));
    }
}
