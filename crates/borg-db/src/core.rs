use anyhow::Result;
use turso::Builder;

use crate::BorgDb;

impl BorgDb {
    pub fn new(conn: turso::Connection) -> Self {
        Self { conn }
    }

    pub async fn open_local(path: &str) -> Result<Self> {
        let db = Builder::new_local(path).build().await?;
        let conn = db.connect()?;
        Ok(Self::new(conn))
    }
}
