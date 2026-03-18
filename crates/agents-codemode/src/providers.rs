use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Source of JavaScript packages available to the engine.
#[async_trait]
pub trait PackageProvider: Send + Sync + 'static {
    async fn fetch(&self) -> Result<Vec<Package>>;
}

/// Source of environment variables exposed to the engine.
#[async_trait]
pub trait EnvironmentProvider: Send + Sync + 'static {
    async fn fetch(&self) -> Result<Vec<EnvironmentVariable>>;
}

/// JavaScript package made available to the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub code: String,
}

/// Environment variable made available to the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}
