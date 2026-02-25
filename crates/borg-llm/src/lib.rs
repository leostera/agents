use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub mod providers;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, model: &str, messages: &[Value], tools: &[Value]) -> Result<Value>;
}
