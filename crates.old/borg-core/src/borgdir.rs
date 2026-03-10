use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::fs::{OpenOptions, create_dir_all};
use tracing::{debug, info, trace};

#[derive(Debug, Clone)]
pub struct BorgDir {
    root: PathBuf,
    logs: PathBuf,
    files: PathBuf,
    assets: PathBuf,
    config_db: PathBuf,
    memory_db: PathBuf,
}

impl Default for BorgDir {
    fn default() -> Self {
        Self::new()
    }
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
        let files = root.join("files");
        let assets = root.join("assets");
        let config_db = root.join("config.db");
        let memory_db = root.join("memory.db");

        Self {
            root,
            logs,
            files,
            assets,
            config_db,
            memory_db,
        }
    }

    pub async fn init(home: Option<String>) -> Result<Self> {
        let borg_dir = Self::from_home(home);
        info!(
            target: "borg_core",
            root = %borg_dir.root.display(),
            "initializing borg directory layout"
        );
        borg_dir.ensure_initialized().await?;
        info!(
            target: "borg_core",
            root = %borg_dir.root.display(),
            "borg directory layout initialized"
        );
        Ok(borg_dir)
    }

    pub async fn ensure_initialized(&self) -> Result<()> {
        debug!(
            target: "borg_core",
            root = %self.root.display(),
            logs = %self.logs.display(),
            files = %self.files.display(),
            assets = %self.assets.display(),
            config_db = %self.config_db.display(),
            memory_db = %self.memory_db.display(),
            "ensuring borg directory structure exists"
        );
        create_dir_all(&self.root).await?;
        create_dir_all(&self.logs).await?;
        create_dir_all(&self.files).await?;
        create_dir_all(&self.assets).await?;

        touch_file(&self.config_db).await?;
        touch_file(&self.memory_db).await?;
        trace!(target: "borg_core", "borg directory ensure_initialized completed");
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

    pub fn memory_db(&self) -> &Path {
        &self.memory_db
    }

    pub fn files(&self) -> &Path {
        &self.files
    }

    pub fn assets(&self) -> &Path {
        &self.assets
    }
}

async fn touch_file(path: &Path) -> Result<()> {
    trace!(target: "borg_core", path = %path.display(), "touching file");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .await?;
    Ok(())
}
