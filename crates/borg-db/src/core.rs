use anyhow::{Context, Result};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use crate::{BorgDb, CompatConn};

impl BorgDb {
    pub fn new(sqlite: SqlitePool) -> Self {
        Self {
            conn: CompatConn::new(sqlite),
        }
    }

    pub async fn open_local(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let sqlite = SqlitePool::connect_with(options)
            .await
            .context("failed to open sqlx sqlite pool")?;

        sqlx::query("PRAGMA busy_timeout = 5000")
            .execute(&sqlite)
            .await
            .context("failed to configure sqlite busy_timeout")?;

        Ok(Self::new(sqlite))
    }
}
