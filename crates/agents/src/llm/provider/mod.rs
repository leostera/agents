pub mod anthropic;
#[cfg(all(feature = "apple", target_os = "macos"))]
pub mod apple;
pub mod lm_studio;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod workers_ai;

use crate::llm::capability::Capability;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::llm::completion::{
    ProviderType, RawCompletionEvent, RawCompletionEventStream, RawCompletionRequest,
    RawCompletionResponse,
};
use crate::llm::error::LlmResult;
use crate::llm::model::Model;
use crate::llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait LlmProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;

    fn provider_name(&self) -> &'static str;

    fn capabilities(&self) -> &[Capability];

    async fn available_models(&self) -> LlmResult<Vec<Model>>;

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse>;

    async fn chat_raw_stream(
        &self,
        mut req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        req.response_mode = crate::llm::completion::ResponseMode::Buffered;
        let response = self.chat_raw(req).await?;
        let (sender, receiver) = mpsc::channel(1);
        let _ = sender.send(Ok(RawCompletionEvent::Done(response))).await;
        Ok(RawCompletionEventStream::new(receiver))
    }

    async fn transcribe(
        &self,
        req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse>;
}
