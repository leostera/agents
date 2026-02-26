use std::path::Path;

use anyhow::{Result, anyhow};
use borg_core::Entity;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

mod entity_graph;
mod fact_store;
mod search_index;
mod tools;

use entity_graph::IndraEntityGraph;
use fact_store::TursoFactStore;
pub use fact_store::{FactInput, FactRecord, FactValue, StateFactsResult, Uri};
pub use fact_store::FactArity;
use search_index::TantivySearchIndex;
pub use tools::{build_memory_toolchain, default_tool_specs as default_memory_tool_specs};

const CONSOLIDATION_BATCH_SIZE: usize = 128;
const COMMAND_BUFFER: usize = 1024;
const CONSOLIDATION_BUFFER: usize = 4096;
const DEFAULT_SEARCH_LIMIT: usize = 25;

#[macro_export]
macro_rules! uri {
    ($ns:expr, $kind:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some(&::uuid::Uuid::now_v7().to_string()))
    };
    ($ns:expr, $kind:expr, $id:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some($id))
    };
}

#[derive(Clone)]
pub struct MemoryStore {
    fact_store: TursoFactStore,
    entity_graph: IndraEntityGraph,
    search_index: TantivySearchIndex,
    consolidate_tx: mpsc::Sender<FactRecord>,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>, search_path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        let search_root = search_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(&search_root)?;

        let fact_store = TursoFactStore::new(&root)?;
        let entity_graph = IndraEntityGraph::new(&root)?;
        let search_index = TantivySearchIndex::new(&search_root)?;
        let (consolidate_tx, consolidate_rx) = mpsc::channel(CONSOLIDATION_BUFFER);

        let store = Self {
            fact_store,
            entity_graph,
            search_index,
            consolidate_tx,
        };
        store.start_consolidator(consolidate_rx);

        info!(
            target: "borg_ltm",
            root = %root.display(),
            search_root = %search_root.display(),
            "initialized memory store with fact_store=turso entity_graph=indradb search_index=tantivy"
        );
        Ok(store)
    }

    pub async fn migrate(&self) -> Result<()> {
        self.fact_store.migrate().await?;
        self.entity_graph.migrate().await?;
        self.search_index.migrate().await?;
        self.replay_pending_projection().await?;
        Ok(())
    }

    pub async fn state_facts(&self, facts: Vec<FactInput>) -> Result<StateFactsResult> {
        let result = self.fact_store.state_facts(facts).await?;
        for fact in result.facts.clone() {
            if self.consolidate_tx.send(fact).await.is_err() {
                return Err(anyhow!("ltm consolidation worker unavailable"));
            }
        }
        Ok(result)
    }

    pub async fn get_entity_uri(&self, entity_uri: &Uri) -> Result<Option<Entity>> {
        self.entity_graph.get_entity(entity_uri.as_str()).await
    }

    pub async fn search_query(&self, query: SearchQuery) -> Result<SearchResults> {
        let limit = query.limit.unwrap_or(DEFAULT_SEARCH_LIMIT).max(1);
        let index_hits = self.search_index.search(&query, limit).await?;

        let mut entities = Vec::new();
        for entity_id in index_hits {
            if let Some(entity) = self.entity_graph.get_entity(&entity_id).await? {
                entities.push(entity);
            }
            if entities.len() >= limit {
                break;
            }
        }

        if entities.is_empty() {
            let fallback_text = query.text().unwrap_or_default();
            if !fallback_text.is_empty() {
                entities = self
                    .entity_graph
                    .search(fallback_text, query.kind.as_deref(), limit)
                    .await?;
                if let Some(ns) = &query.ns {
                    entities.retain(|entity| {
                        entity
                            .props
                            .get("namespace")
                            .and_then(|value| value.as_str())
                            == Some(ns.as_str())
                    });
                }
            }
        }

        Ok(SearchResults { entities })
    }

    pub async fn upsert_entity(
        &self,
        entity_type: &str,
        label: &str,
        props: &serde_json::Value,
        natural_key: Option<&str>,
    ) -> Result<String> {
        self.entity_graph
            .upsert_entity(entity_type, label, props, natural_key)
            .await
    }

    pub async fn link(
        &self,
        from_entity_id: &str,
        rel_type: &str,
        to_entity_id: &str,
        props: &serde_json::Value,
    ) -> Result<String> {
        self.entity_graph
            .link(from_entity_id, rel_type, to_entity_id, props)
            .await
    }

    pub async fn get_entity(&self, entity_id: &str) -> Result<Option<Entity>> {
        self.entity_graph.get_entity(entity_id).await
    }

    pub async fn search(
        &self,
        text: &str,
        entity_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Entity>> {
        let query = SearchQuery {
            ns: None,
            kind: entity_type.map(ToOwned::to_owned),
            name: Some(NameFilter {
                like: text.to_string(),
            }),
            query_text: Some(text.to_string()),
            limit: Some(limit),
        };
        Ok(self.search_query(query).await?.entities)
    }

    fn start_consolidator(&self, mut rx: mpsc::Receiver<FactRecord>) {
        let graph = self.entity_graph.clone();
        let index = self.search_index.clone();
        let facts = self.fact_store.clone();
        tokio::spawn(async move {
            while let Some(fact) = rx.recv().await {
                if let Err(err) = apply_projection(&graph, &index, &facts, fact).await {
                    warn!(target: "borg_ltm", error = %err, "failed to project stated fact");
                }
            }
        });
    }

    async fn replay_pending_projection(&self) -> Result<()> {
        let pending = self
            .fact_store
            .dequeue_projection_batch(CONSOLIDATION_BATCH_SIZE)
            .await?;
        if pending.is_empty() {
            return Ok(());
        }
        debug!(target: "borg_ltm", pending = pending.len(), "replaying pending projection facts");
        for fact in pending {
            self.consolidate_tx
                .send(fact)
                .await
                .map_err(|_| anyhow!("ltm consolidation worker unavailable"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameFilter {
    pub like: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub ns: Option<String>,
    pub kind: Option<String>,
    pub name: Option<NameFilter>,
    #[serde(rename = "q")]
    pub query_text: Option<String>,
    pub limit: Option<usize>,
}

impl SearchQuery {
    pub(crate) fn text(&self) -> Option<&str> {
        if let Some(name) = &self.name {
            if !name.like.trim().is_empty() {
                return Some(name.like.as_str());
            }
        }
        self.query_text
            .as_deref()
            .filter(|query| !query.trim().is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub entities: Vec<Entity>,
}

pub struct BorgLtmServer {
    store: MemoryStore,
    command_rx: mpsc::Receiver<Command>,
}

#[derive(Clone)]
pub struct BorgLtm {
    command_tx: mpsc::Sender<Command>,
}

impl BorgLtmServer {
    pub fn new(path: impl AsRef<Path>, search_path: impl AsRef<Path>) -> Result<(Self, BorgLtm)> {
        let store = MemoryStore::new(path, search_path)?;
        let (command_tx, command_rx) = mpsc::channel(COMMAND_BUFFER);
        Ok((Self { store, command_rx }, BorgLtm { command_tx }))
    }

    pub async fn run(mut self) -> Result<()> {
        self.store.migrate().await?;
        info!(target: "borg_ltm", "ltm server started");

        while let Some(command) = self.command_rx.recv().await {
            match command {
                Command::StateFacts { facts, respond_to } => {
                    let _ = respond_to.send(self.store.state_facts(facts).await);
                }
                Command::GetEntity {
                    entity_uri,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.store.get_entity_uri(&entity_uri).await);
                }
                Command::Search { query, respond_to } => {
                    let _ = respond_to.send(self.store.search_query(query).await);
                }
            }
        }

        info!(target: "borg_ltm", "ltm server stopped");
        Ok(())
    }
}

impl BorgLtm {
    pub async fn state_facts(&self, facts: Vec<FactInput>) -> Result<StateFactsResult> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::StateFacts {
                facts,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("ltm server unavailable"))?;
        rx.await.map_err(|_| anyhow!("ltm server unavailable"))?
    }

    pub async fn get_entity(&self, entity_uri: Uri) -> Result<Option<Entity>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::GetEntity {
                entity_uri,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("ltm server unavailable"))?;
        rx.await.map_err(|_| anyhow!("ltm server unavailable"))?
    }

    pub async fn search(&self, query: SearchQuery) -> Result<SearchResults> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::Search {
                query,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("ltm server unavailable"))?;
        rx.await.map_err(|_| anyhow!("ltm server unavailable"))?
    }
}

enum Command {
    StateFacts {
        facts: Vec<FactInput>,
        respond_to: oneshot::Sender<Result<StateFactsResult>>,
    },
    GetEntity {
        entity_uri: Uri,
        respond_to: oneshot::Sender<Result<Option<Entity>>>,
    },
    Search {
        query: SearchQuery,
        respond_to: oneshot::Sender<Result<SearchResults>>,
    },
}

async fn apply_projection(
    graph: &IndraEntityGraph,
    index: &TantivySearchIndex,
    facts: &TursoFactStore,
    fact: FactRecord,
) -> Result<()> {
    graph.apply_fact(&fact).await?;
    if let Some(entity) = graph.get_entity(fact.entity.as_str()).await? {
        index.upsert_entity(&entity).await?;
    }
    facts.mark_projected(fact.fact_id.as_str()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::time::{Duration, timeout};
    use uuid::Uuid;

    fn temp_paths(prefix: &str) -> (PathBuf, PathBuf) {
        let root = PathBuf::from(format!("/tmp/{}-{}", prefix, Uuid::now_v7()));
        let search = PathBuf::from(format!("/tmp/{}-search-{}", prefix, Uuid::now_v7()));
        (root, search)
    }

    fn make_fact(entity: Uri, field: &str, value: FactValue) -> FactInput {
        make_fact_with_arity(entity, field, FactArity::One, value)
    }

    fn make_fact_with_arity(
        entity: Uri,
        field: &str,
        arity: FactArity,
        value: FactValue,
    ) -> FactInput {
        FactInput {
            source: uri!("borg", "session").unwrap(),
            entity,
            field: Uri::parse(field).unwrap(),
            arity,
            value,
        }
    }

    async fn wait_until_entity(ltm: &BorgLtm, entity: Uri) -> Option<Entity> {
        for _ in 0..40 {
            let found = ltm.get_entity(entity.clone()).await.unwrap();
            if found.is_some() {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        None
    }

    #[tokio::test]
    async fn state_facts_persists_and_can_query_entity() {
        let (root, search) = temp_paths("borg-ltm-test");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let entity = uri!("plex", "movies").unwrap();
        ltm.state_facts(vec![make_fact(
            entity.clone(),
            "borg:fields:name",
            FactValue::Text("Minions".to_string()),
        )])
        .await
        .unwrap();

        let found = wait_until_entity(&ltm, entity.clone()).await;
        assert!(found.is_some());

        let results = ltm
            .search(SearchQuery {
                ns: Some("plex".to_string()),
                kind: Some("movies".to_string()),
                name: Some(NameFilter {
                    like: "MINIONS".to_string(),
                }),
                query_text: None,
                limit: Some(5),
            })
            .await
            .unwrap();
        assert!(!results.entities.is_empty());
    }

    #[tokio::test]
    async fn server_exits_when_all_handles_are_dropped() {
        let (root, search) = temp_paths("borg-ltm-server-exit");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        let task = tokio::spawn(async move { server.run().await.unwrap() });
        drop(ltm);
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn concurrent_state_facts_are_processed() {
        let (root, search) = temp_paths("borg-ltm-concurrent");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let mut jobs = Vec::new();
        for i in 0..10 {
            let ltm_clone = ltm.clone();
            jobs.push(tokio::spawn(async move {
                let entity = uri!("plex", "movies").unwrap();
                ltm_clone
                    .state_facts(vec![make_fact(
                        entity,
                        "borg:fields:name",
                        FactValue::Text(format!("Movie {}", i)),
                    )])
                    .await
                    .unwrap();
            }));
        }
        for job in jobs {
            job.await.unwrap();
        }

        let results = ltm
            .search(SearchQuery {
                ns: Some("plex".to_string()),
                kind: Some("movies".to_string()),
                name: Some(NameFilter {
                    like: "Movie".to_string(),
                }),
                query_text: None,
                limit: Some(20),
            })
            .await
            .unwrap();
        assert!(!results.entities.is_empty());
    }

    #[tokio::test]
    async fn merges_multiple_facts_into_single_entity_projection() {
        let (root, search) = temp_paths("borg-ltm-merge");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let entity = uri!("plex", "movies").unwrap();
        ltm.state_facts(vec![
            make_fact(
                entity.clone(),
                "borg:fields:name",
                FactValue::Text("Minions".to_string()),
            ),
            make_fact(entity.clone(), "borg:fields:year", FactValue::Integer(2015)),
        ])
        .await
        .unwrap();

        let found = wait_until_entity(&ltm, entity.clone()).await.unwrap();
        assert_eq!(found.label, "Minions");
        assert_eq!(found.props.get("year").and_then(|v| v.as_i64()), Some(2015));
    }

    #[tokio::test]
    async fn many_arity_accumulates_distinct_values_into_array() {
        let (root, search) = temp_paths("borg-ltm-many-arity");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let entity = uri!("borg", "user", "leostera").unwrap();
        ltm.state_facts(vec![
            make_fact_with_arity(
                entity.clone(),
                "borg:preference:hobby",
                FactArity::Many,
                FactValue::Text("climbing".to_string()),
            ),
            make_fact_with_arity(
                entity.clone(),
                "borg:preference:hobby",
                FactArity::Many,
                FactValue::Text("cooking".to_string()),
            ),
            make_fact_with_arity(
                entity.clone(),
                "borg:preference:hobby",
                FactArity::Many,
                FactValue::Text("climbing".to_string()),
            ),
        ])
        .await
        .unwrap();

        let found = wait_until_entity(&ltm, entity).await.unwrap();
        let hobbies = found
            .props
            .get("hobby")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        assert_eq!(hobbies.len(), 2);
        assert!(hobbies.contains(&serde_json::Value::String("climbing".to_string())));
        assert!(hobbies.contains(&serde_json::Value::String("cooking".to_string())));
    }

    #[tokio::test]
    async fn replay_pending_projection_on_restart() {
        let (root, search) = temp_paths("borg-ltm-replay");

        let fact_store = super::fact_store::TursoFactStore::new(&root).unwrap();
        fact_store.migrate().await.unwrap();
        let entity = uri!("plex", "movies").unwrap();
        let _ = fact_store
            .state_facts(vec![make_fact(
                entity.clone(),
                "borg:fields:name",
                FactValue::Text("Replayed".to_string()),
            )])
            .await
            .unwrap();

        let memory = MemoryStore::new(&root, &search).unwrap();
        memory.migrate().await.unwrap();

        let mut found = None;
        for _ in 0..40 {
            match memory.get_entity(entity.as_str()).await {
                Ok(value) => {
                    found = value;
                    if found.is_some() {
                        break;
                    }
                }
                Err(_) => {
                    // Projection can be mid-write; retry.
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn search_filters_namespace_and_kind() {
        let (root, search) = temp_paths("borg-ltm-filter");
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let movie = uri!("plex", "movies").unwrap();
        let album = uri!("plex", "albums").unwrap();
        ltm.state_facts(vec![
            make_fact(
                movie.clone(),
                "borg:fields:name",
                FactValue::Text("Minions".to_string()),
            ),
            make_fact(
                album.clone(),
                "borg:fields:name",
                FactValue::Text("Minions Soundtrack".to_string()),
            ),
        ])
        .await
        .unwrap();

        let _ = wait_until_entity(&ltm, movie.clone()).await;
        let results = ltm
            .search(SearchQuery {
                ns: Some("plex".to_string()),
                kind: Some("movies".to_string()),
                name: Some(NameFilter {
                    like: "Minions".to_string(),
                }),
                query_text: None,
                limit: Some(10),
            })
            .await
            .unwrap();

        assert!(results.entities.iter().all(|entity| {
            entity
                .props
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                == "movies"
        }));
    }

    #[tokio::test]
    async fn search_index_persists_across_server_restart() {
        let (root, search) = temp_paths("borg-ltm-search-persist");

        let (server_a, ltm_a) = BorgLtmServer::new(&root, &search).unwrap();
        let task_a = tokio::spawn(async move {
            server_a.run().await.unwrap();
        });

        let entity = uri!("plex", "movies").unwrap();
        ltm_a
            .state_facts(vec![make_fact(
                entity.clone(),
                "borg:fields:name",
                FactValue::Text("Persistent Minions".to_string()),
            )])
            .await
            .unwrap();

        let _ = wait_until_entity(&ltm_a, entity.clone()).await;
        let before_restart = ltm_a
            .search(SearchQuery {
                ns: Some("plex".to_string()),
                kind: Some("movies".to_string()),
                name: Some(NameFilter {
                    like: "Persistent Minions".to_string(),
                }),
                query_text: None,
                limit: Some(10),
            })
            .await
            .unwrap();
        assert!(!before_restart.entities.is_empty());

        drop(ltm_a);
        timeout(Duration::from_secs(2), task_a)
            .await
            .unwrap()
            .unwrap();

        let (server_b, ltm_b) = BorgLtmServer::new(&root, &search).unwrap();
        let task_b = tokio::spawn(async move {
            server_b.run().await.unwrap();
        });

        let mut after_restart = SearchResults { entities: vec![] };
        for _ in 0..30 {
            after_restart = ltm_b
                .search(SearchQuery {
                    ns: Some("plex".to_string()),
                    kind: Some("movies".to_string()),
                    name: Some(NameFilter {
                        like: "Persistent Minions".to_string(),
                    }),
                    query_text: None,
                    limit: Some(10),
                })
                .await
                .unwrap();
            if !after_restart.entities.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        assert!(!after_restart.entities.is_empty());
        assert!(
            after_restart
                .entities
                .iter()
                .any(|candidate| candidate.entity_id.as_str() == entity.as_str())
        );

        drop(ltm_b);
        timeout(Duration::from_secs(2), task_b)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn uri_macro_builds_valid_values() {
        let generated = uri!("plex", "movies").unwrap();
        let explicit = uri!("plex", "movies", "abc123").unwrap();
        assert!(generated.as_str().starts_with("plex:movies:"));
        assert_eq!(explicit.as_str(), "plex:movies:abc123");
    }

    #[test]
    fn uri_parse_validation_checks_shape() {
        assert!(Uri::parse("plex:movies:abc").is_ok());
        assert!(Uri::parse("plex:movies").is_err());
        assert!(Uri::parse("plex::id").is_err());
        assert!(Uri::parse("not a uri").is_err());
    }
}
