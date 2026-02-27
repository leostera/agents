use anyhow::{Context, Result};
use turso::Builder;

use crate::BorgDb;

impl BorgDb {
    pub fn new(conn: turso::Connection) -> Self {
        Self { conn }
    }

    pub async fn open_local(path: &str) -> Result<Self> {
        let db = Builder::new_local(path).build().await?;
        let conn = db.connect()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            "#,
        )
        .await
        .context("failed to configure sqlite pragmas")?;
        Ok(Self::new(conn))
    }
}
