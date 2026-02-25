use std::path::Path;

use anyhow::{Result, anyhow};
use borg_core::Entity;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

mod entity_graph;
mod fact_store;
mod search_index;

use entity_graph::IndraEntityGraph;
use fact_store::TursoFactStore;
pub use fact_store::{FactInput, FactRecord, FactValue, StateFactsResult, Uri};
use search_index::TantivySearchIndex;

const CONSOLIDATION_BATCH_SIZE: usize = 128;
const COMMAND_BUFFER: usize = 1024;
const CONSOLIDATION_BUFFER: usize = 4096;
const DEFAULT_SEARCH_LIMIT: usize = 25;

#[macro_export]
macro_rules! uri {
    ($ns:expr, $kind:expr) => {
        $crate::Uri::from_parts($ns, $kind, None)
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
    use uuid::Uuid;

    #[tokio::test]
    async fn state_facts_persists_and_can_query_entity() {
        let root = PathBuf::from(format!("/tmp/borg-ltm-test-{}", Uuid::now_v7()));
        let search = PathBuf::from(format!("/tmp/borg-ltm-search-test-{}", Uuid::now_v7()));
        let (server, ltm) = BorgLtmServer::new(&root, &search).unwrap();
        tokio::spawn(async move {
            server.run().await.unwrap();
        });

        let source = Uri::parse(format!("borg:session:{}", Uuid::now_v7())).unwrap();
        let entity = Uri::parse(format!("plex:movies:{}", Uuid::now_v7())).unwrap();
        let field = Uri::parse("borg:fields:name").unwrap();
        ltm.state_facts(vec![FactInput {
            source,
            entity: entity.clone(),
            field,
            value: FactValue::Text("Minions".to_string()),
        }])
        .await
        .unwrap();

        let mut found = None;
        for _ in 0..30 {
            found = ltm.get_entity(entity.clone()).await.unwrap();
            if found.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
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

    #[test]
    fn uri_macro_builds_valid_values() {
        let generated = uri!("plex", "movies").unwrap();
        let explicit = uri!("plex", "movies", "abc123").unwrap();
        assert!(generated.as_str().starts_with("plex:movies:"));
        assert_eq!(explicit.as_str(), "plex:movies:abc123");
    }
}
