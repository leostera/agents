use std::path::Path;

use anyhow::Result;
use borg_core::Entity;
use serde_json::Value;
use tracing::info;

mod entity_graph;
mod fact_store;

use entity_graph::IndraEntityGraph;
use fact_store::TursoFactStore;

#[derive(Clone)]
pub struct MemoryStore {
    fact_store: TursoFactStore,
    entity_graph: IndraEntityGraph,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;

        let fact_store = TursoFactStore::new(&root)?;
        let entity_graph = IndraEntityGraph::new(&root)?;

        info!(
            target: "borg_ltm",
            root = %root.display(),
            "initialized memory store with fact_store=turso and entity_graph=indradb"
        );

        Ok(Self {
            fact_store,
            entity_graph,
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        self.fact_store.migrate().await?;
        self.entity_graph.migrate().await?;
        Ok(())
    }

    pub async fn upsert_entity(
        &self,
        entity_type: &str,
        label: &str,
        props: &Value,
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
        props: &Value,
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
        self.entity_graph.search(text, entity_type, limit).await
    }
}
