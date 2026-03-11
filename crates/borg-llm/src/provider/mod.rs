pub mod anthropic;
pub mod apple;
pub mod lm_studio;
pub mod ollama;
pub mod openai;
pub mod openrouter;

use async_trait::async_trait;

use crate::capability::Capability;
use tokio::sync::mpsc;

use crate::completion::{
    ProviderType, RawCompletionEvent, RawCompletionEventStream, RawCompletionRequest,
    RawCompletionResponse,
};
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

    async fn chat_raw_stream(
        &self,
        mut req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        req.response_mode = crate::completion::ResponseMode::Buffered;
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
