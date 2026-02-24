use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::fs::{OpenOptions, create_dir_all};

#[derive(Debug, Clone)]
pub struct BorgDir {
    root: PathBuf,
    logs: PathBuf,
    config_db: PathBuf,
    ltm_db: PathBuf,
}

impl BorgDir {
    pub fn new() -> Self {
        Self::from_home(None)
    }

    pub fn from_home(home: Option<String>) -> Self {
        let home_dir = match home {
            Some(home) => PathBuf::from(home),
            None => PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())),
        };

        let root = home_dir.join(".borg");
        let logs = root.join("logs");
        let config_db = root.join("config.db");
        let ltm_db = root.join("ltm.db");

        Self {
            root,
            logs,
            config_db,
            ltm_db,
        }
    }

    pub async fn init(home: Option<String>) -> Result<Self> {
        let borg_dir = Self::from_home(home);
        borg_dir.ensure_initialized().await?;
        Ok(borg_dir)
    }

    pub async fn ensure_initialized(&self) -> Result<()> {
        create_dir_all(&self.root).await?;
        create_dir_all(&self.logs).await?;
        create_dir_all(&self.ltm_db).await?;

        touch_file(&self.config_db).await?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn logs(&self) -> &Path {
        &self.logs
    }

    pub fn config_db(&self) -> &Path {
        &self.config_db
    }

    pub fn ltm_db(&self) -> &Path {
        &self.ltm_db
    }
}

async fn touch_file(path: &Path) -> Result<()> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .await?;
    Ok(())
}
