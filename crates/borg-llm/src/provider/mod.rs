pub mod anthropic;
pub mod lm_studio;
pub mod ollama;
pub mod openai;
pub mod openrouter;

use async_trait::async_trait;

use crate::capability::Capability;
use crate::completion::{ProviderType, RawCompletionRequest, RawCompletionResponse};
use crate::error::LlmResult;
use crate::model::Model;
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;

    fn provider_name(&self) -> &'static str;

    fn capabilities(&self) -> &[Capability];

    async fn available_models(&self) -> LlmResult<Vec<Model>>;

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse>;

    async fn transcribe(
        &self,
        req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse>;
}
