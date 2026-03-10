pub mod openai;
pub mod anthropic;
pub mod openrouter;
pub mod lm_studio;
pub mod ollama;

use crate::model::Model;
use crate::error::LlmResult;
use crate::capability::Capability;

use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    fn capabilities(&self) -> &[Capability];

    async fn available_models(&self) -> LlmResult<Vec<Model>>;
}
