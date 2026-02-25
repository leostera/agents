use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::info;
use turso::Builder;

const FACTS_DB_FILE: &str = "facts.db";

#[derive(Clone)]
pub(crate) struct TursoFactStore {
    db_path: PathBuf,
}

impl TursoFactStore {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        let db_path = root.join(FACTS_DB_FILE);
        Ok(Self { db_path })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        info!(
            target: "borg_ltm",
            path = %self.db_path.display(),
            "running fact-store migrations"
        );

        let db = Builder::new_local(self.db_path.to_string_lossy().as_ref())
            .build()
            .await?;
        let conn = db.connect()?;

        conn
            .execute_batch(
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
                "#,
            )
            .await?;

        info!(target: "borg_ltm", "fact-store migrations completed");
        Ok(())
    }
}
