use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use turso::{Builder, Connection};
use url::Url;
use uuid::Uuid;

const FACTS_DB_FILE: &str = "facts.db";
const FACTS_TABLE: &str = "ltm_facts";
const QUEUED_STATUS: &str = "queued";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uri(Url);

impl Uri {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let parsed = Url::parse(&value)?;
        let opaque = parsed.path().trim_start_matches('/');
        let mut parts = opaque.split(':');
        let kind = parts
            .next()
            .ok_or_else(|| anyhow!("invalid uri: {}", value))?;
        let id = parts
            .next()
            .ok_or_else(|| anyhow!("invalid uri: {}", value))?;
        if parts.next().is_some() || parsed.scheme().is_empty() || kind.is_empty() || id.is_empty()
        {
            return Err(anyhow!("invalid uri: {}", value));
        }
        Ok(Self(parsed))
    }

    pub fn from_parts(ns: &str, kind: &str, id: Option<&str>) -> Result<Self> {
        if ns.is_empty() || kind.is_empty() {
            return Err(anyhow!("invalid uri parts"));
        }
        let id = id
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        Self::parse(format!("{}:{}:{}", ns, kind, id))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Ref(Uri),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactInput {
    pub source: Uri,
    pub entity: Uri,
    pub field: Uri,
    pub value: FactValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactRecord {
    pub fact_id: Uri,
    pub source: Uri,
    pub entity: Uri,
    pub field: Uri,
    pub value: FactValue,
    pub tx_id: Uri,
    pub stated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFactsResult {
    pub tx_id: Uri,
    pub facts: Vec<FactRecord>,
}

#[derive(Clone)]
pub(crate) struct TursoFactStore {
    db_path: PathBuf,
    schema_ready: Arc<AtomicBool>,
    schema_lock: Arc<tokio::sync::Mutex<()>>,
}

impl TursoFactStore {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let db_path = root.join(FACTS_DB_FILE);
        Ok(Self {
            db_path,
            schema_ready: Arc::new(AtomicBool::new(false)),
            schema_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        self.ensure_migrated().await
    }

    pub(crate) async fn state_facts(&self, facts: Vec<FactInput>) -> Result<StateFactsResult> {
        self.ensure_migrated().await?;
        if facts.is_empty() {
            return Err(anyhow!("state_facts received empty input"));
        }

        let conn = self.open_conn().await?;
        let tx_id = Uri::parse(format!("borg:tx:{}", Uuid::now_v7()))?;
        let stated_at = Utc::now();
        let stated_at_str = stated_at.to_rfc3339();
        let enqueued_at = Utc::now().to_rfc3339();

        let mut out = Vec::with_capacity(facts.len());
        for fact in facts {
            let fact_id = Uri::parse(format!("borg:fact:{}", Uuid::now_v7()))?;
            let (
                value_kind,
                value_text,
                value_int,
                value_float,
                value_bool,
                value_bytes,
                value_ref,
            ) = encode_value(&fact.value);

            conn.execute(
                &format!(
                    "INSERT INTO {}(fact_id, source, entity, field, value_kind, value_text, value_int, value_float, value_bool, value_bytes, value_ref, tx_id, stated_at, retracted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)",
                    FACTS_TABLE
                ),
                (
                    fact_id.to_string(),
                    fact.source.to_string(),
                    fact.entity.to_string(),
                    fact.field.to_string(),
                    value_kind.to_string(),
                    value_text.clone(),
                    value_int,
                    value_float,
                    value_bool,
                    value_bytes.clone(),
                    value_ref.clone(),
                    tx_id.to_string(),
                    stated_at_str.clone(),
                ),
            )
            .await?;

            conn.execute(
                "INSERT OR REPLACE INTO ltm_projection_queue(fact_id, status, enqueued_at) VALUES (?1, ?2, ?3)",
                (fact_id.to_string(), QUEUED_STATUS.to_string(), enqueued_at.clone()),
            )
            .await?;

            out.push(FactRecord {
                fact_id,
                source: fact.source,
                entity: fact.entity,
                field: fact.field,
                value: fact.value,
                tx_id: tx_id.clone(),
                stated_at,
            });
        }

        Ok(StateFactsResult { tx_id, facts: out })
    }

    pub(crate) async fn load_fact(&self, fact_id: &str) -> Result<Option<FactRecord>> {
        self.ensure_migrated().await?;
        let conn = self.open_conn().await?;
        let mut rows = conn
            .query(
                &format!("SELECT fact_id, source, entity, field, value_kind, value_text, value_int, value_float, value_bool, value_bytes, value_ref, tx_id, stated_at FROM {} WHERE fact_id = ?1", FACTS_TABLE),
                (fact_id.to_string(),),
            )
            .await?;

        if let Some(row) = rows.next().await? {
            return Ok(Some(row_to_fact(&row)?));
        }
        Ok(None)
    }

    pub(crate) async fn dequeue_projection_batch(&self, limit: usize) -> Result<Vec<FactRecord>> {
        self.ensure_migrated().await?;
        let conn = self.open_conn().await?;
        let mut rows = conn
            .query(
                "SELECT fact_id FROM ltm_projection_queue WHERE status = ?1 ORDER BY enqueued_at ASC LIMIT ?2",
                (QUEUED_STATUS.to_string(), i64::try_from(limit).unwrap_or(128)),
            )
            .await?;

        let mut facts = Vec::new();
        while let Some(row) = rows.next().await? {
            let fact_id: String = row.get(0)?;
            if let Some(fact) = self.load_fact(&fact_id).await? {
                facts.push(fact);
            }
        }
        Ok(facts)
    }

    pub(crate) async fn mark_projected(&self, fact_id: &str) -> Result<()> {
        self.ensure_migrated().await?;
        let conn = self.open_conn().await?;
        conn.execute(
            "DELETE FROM ltm_projection_queue WHERE fact_id = ?1",
            (fact_id.to_string(),),
        )
        .await?;
        Ok(())
    }

    async fn ensure_migrated(&self) -> Result<()> {
        if self.schema_ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = self.schema_lock.lock().await;
        if self.schema_ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.migrate_inner().await?;
        self.schema_ready.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn migrate_inner(&self) -> Result<()> {
        info!(
            target: "borg_ltm",
            path = %self.db_path.display(),
            "running fact-store migrations"
        );

        let conn = self.open_conn().await?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS ltm_facts (
                fact_id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                entity TEXT NOT NULL,
                field TEXT NOT NULL,
                value_kind TEXT NOT NULL,
                value_text TEXT,
                value_int INTEGER,
                value_float REAL,
                value_bool INTEGER,
                value_bytes BLOB,
                value_ref TEXT,
                tx_id TEXT NOT NULL,
                stated_at TEXT NOT NULL,
                retracted_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_ltm_facts_entity ON ltm_facts(entity);
            CREATE INDEX IF NOT EXISTS idx_ltm_facts_source ON ltm_facts(source);
            CREATE INDEX IF NOT EXISTS idx_ltm_facts_tx ON ltm_facts(tx_id);
            CREATE INDEX IF NOT EXISTS idx_ltm_facts_field ON ltm_facts(field);

            CREATE TABLE IF NOT EXISTS ltm_projection_queue (
                fact_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                enqueued_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ltm_projection_queue_status ON ltm_projection_queue(status);
            "#
        ).await?;

        info!(target: "borg_ltm", "fact-store migrations completed");
        Ok(())
    }

    async fn open_conn(&self) -> Result<Connection> {
        let db = Builder::new_local(self.db_path.to_string_lossy().as_ref())
            .build()
            .await?;
        Ok(db.connect()?)
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn encode_value(
    value: &FactValue,
) -> (
    &'static str,
    Option<String>,
    Option<i64>,
    Option<f64>,
    Option<i64>,
    Option<Vec<u8>>,
    Option<String>,
) {
    match value {
        FactValue::Text(v) => ("text", Some(v.clone()), None, None, None, None, None),
        FactValue::Integer(v) => ("int", None, Some(*v), None, None, None, None),
        FactValue::Float(v) => ("float", None, None, Some(*v), None, None, None),
        FactValue::Boolean(v) => (
            "bool",
            None,
            None,
            None,
            Some(if *v { 1 } else { 0 }),
            None,
            None,
        ),
        FactValue::Bytes(v) => ("bytes", None, None, None, None, Some(v.clone()), None),
        FactValue::Ref(v) => ("ref", None, None, None, None, None, Some(v.to_string())),
    }
}

fn decode_value(
    kind: &str,
    value_text: Option<String>,
    value_int: Option<i64>,
    value_float: Option<f64>,
    value_bool: Option<i64>,
    value_bytes: Option<Vec<u8>>,
    value_ref: Option<String>,
) -> Result<FactValue> {
    match kind {
        "text" => Ok(FactValue::Text(value_text.unwrap_or_default())),
        "int" => Ok(FactValue::Integer(value_int.unwrap_or_default())),
        "float" => Ok(FactValue::Float(value_float.unwrap_or_default())),
        "bool" => Ok(FactValue::Boolean(value_bool.unwrap_or_default() == 1)),
        "bytes" => Ok(FactValue::Bytes(value_bytes.unwrap_or_default())),
        "ref" => Ok(FactValue::Ref(Uri::parse(value_ref.unwrap_or_default())?)),
        _ => Err(anyhow!("unsupported fact value kind: {}", kind)),
    }
}

fn row_to_fact(row: &turso::Row) -> Result<FactRecord> {
    let stated_at_raw: String = row.get(12)?;
    let stated_at = DateTime::parse_from_rfc3339(&stated_at_raw)?.with_timezone(&Utc);
    let kind: String = row.get(4)?;
    let value = decode_value(
        &kind,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    )?;

    Ok(FactRecord {
        fact_id: Uri::parse(row.get::<String>(0)?)?,
        source: Uri::parse(row.get::<String>(1)?)?,
        entity: Uri::parse(row.get::<String>(2)?)?,
        field: Uri::parse(row.get::<String>(3)?)?,
        value,
        tx_id: Uri::parse(row.get::<String>(11)?)?,
        stated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_root() -> PathBuf {
        let root = PathBuf::from(format!("/tmp/borg-ltm-fact-store-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn make_fact(value: FactValue) -> FactInput {
        FactInput {
            source: Uri::parse(format!("borg:session:{}", Uuid::now_v7())).unwrap(),
            entity: Uri::parse(format!("plex:movies:{}", Uuid::now_v7())).unwrap(),
            field: Uri::parse("borg:fields:name").unwrap(),
            value,
        }
    }

    #[test]
    fn uri_parse_rejects_invalid_values() {
        assert!(Uri::parse("not-a-uri").is_err());
        assert!(Uri::parse("borg:missing").is_err());
        assert!(Uri::parse("borg::id").is_err());
        assert!(Uri::parse("://").is_err());
    }

    #[tokio::test]
    async fn state_facts_persists_and_loads_records() {
        let root = make_root();
        let store = TursoFactStore::new(&root).unwrap();
        store.migrate().await.unwrap();

        let result = store
            .state_facts(vec![
                make_fact(FactValue::Text("Minions".to_string())),
                make_fact(FactValue::Integer(42)),
            ])
            .await
            .unwrap();

        assert_eq!(result.facts.len(), 2);
        assert_eq!(result.facts[0].tx_id.to_string(), result.tx_id.to_string());
        assert_eq!(result.facts[1].tx_id.to_string(), result.tx_id.to_string());

        let loaded = store
            .load_fact(result.facts[0].fact_id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.fact_id.to_string(), result.facts[0].fact_id.to_string());
    }

    #[tokio::test]
    async fn roundtrips_all_fact_value_variants() {
        let root = make_root();
        let store = TursoFactStore::new(&root).unwrap();
        store.migrate().await.unwrap();

        let values = vec![
            FactValue::Text("hello".to_string()),
            FactValue::Integer(7),
            FactValue::Float(3.14),
            FactValue::Boolean(true),
            FactValue::Bytes(vec![1, 2, 3]),
            FactValue::Ref(Uri::parse("plex:movies:abc").unwrap()),
        ];

        for value in values {
            let result = store.state_facts(vec![make_fact(value.clone())]).await.unwrap();
            let loaded = store
                .load_fact(result.facts[0].fact_id.as_str())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::to_string(&loaded.value).unwrap(),
                serde_json::to_string(&value).unwrap()
            );
        }
    }

    #[tokio::test]
    async fn projection_queue_dequeue_and_mark_projected() {
        let root = make_root();
        let store = TursoFactStore::new(&root).unwrap();
        store.migrate().await.unwrap();

        let result = store
            .state_facts(vec![
                make_fact(FactValue::Text("one".to_string())),
                make_fact(FactValue::Text("two".to_string())),
            ])
            .await
            .unwrap();

        let queued = store.dequeue_projection_batch(10).await.unwrap();
        assert_eq!(queued.len(), 2);

        store
            .mark_projected(result.facts[0].fact_id.as_str())
            .await
            .unwrap();
        let queued_after = store.dequeue_projection_batch(10).await.unwrap();
        assert_eq!(queued_after.len(), 1);
    }

    #[tokio::test]
    async fn facts_are_durable_across_reopen() {
        let root = make_root();
        let store_a = TursoFactStore::new(&root).unwrap();
        store_a.migrate().await.unwrap();
        let result = store_a
            .state_facts(vec![make_fact(FactValue::Text("persisted".to_string()))])
            .await
            .unwrap();

        let store_b = TursoFactStore::new(&root).unwrap();
        store_b.migrate().await.unwrap();
        let loaded = store_b
            .load_fact(result.facts[0].fact_id.as_str())
            .await
            .unwrap();
        assert!(loaded.is_some());
    }
}
