use anyhow::{Context, Result};
use tracing::info;

use crate::BorgDb;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl BorgDb {
    pub async fn migrate(&self) -> Result<()> {
        info!(target: "borg_db", "running sqlx migrations");
        MIGRATOR
            .run(self.conn.pool())
            .await
            .context("failed to run sqlx migrations")?;
        info!(target: "borg_db", "sqlx migrations completed");
        Ok(())
    }
}
