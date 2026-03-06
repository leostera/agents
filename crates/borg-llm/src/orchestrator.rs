use std::sync::Arc;

use async_trait::async_trait;

use crate::{LlmAssistantMessage, LlmError, LlmRequest, Provider, Result, TranscriptionRequest};

#[derive(Clone)]
#[allow(clippy::upper_case_acronyms)]
pub struct BorgLLM {
    providers: Vec<Arc<dyn Provider>>,
}

impl BorgLLM {
    pub fn build() -> BorgLLMBuilder {
        BorgLLMBuilder::new()
    }

    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self {
        Self { providers }
    }

    pub async fn chat_completion(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let mut last_error: Option<LlmError> = None;

        for provider in &self.providers {
            if !provider.supports_chat_completion() {
                continue;
            }

            match provider.chat(req).await {
                Ok(message) => return Ok(message),
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            LlmError::configuration("no configured provider could satisfy chat completion")
        }))
    }

    pub async fn audio_transcription(&self, req: &TranscriptionRequest) -> Result<String> {
        let mut last_error: Option<LlmError> = None;

        for provider in &self.providers {
            if !provider.supports_audio_transcription() {
                continue;
            }

            match provider.transcribe(req).await {
                Ok(text) => return Ok(text),
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            LlmError::configuration("no configured provider could satisfy audio transcription")
        }))
    }
}

#[async_trait]
impl Provider for BorgLLM {
    fn provider_name(&self) -> &'static str {
        "llm"
    }

    fn supports_chat_completion(&self) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.supports_chat_completion())
    }

    fn supports_audio_transcription(&self) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.supports_audio_transcription())
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        self.chat_completion(req).await
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        self.audio_transcription(req).await
    }
}

pub struct BorgLLMBuilder {
    providers: Vec<Arc<dyn Provider>>,
}

impl BorgLLMBuilder {
    pub fn new() -> Self {
        Self { providers: vec![] }
    }

    pub fn add_provider<P>(mut self, provider: P) -> Self
    where
        P: Provider + 'static,
    {
        self.providers.push(Arc::new(provider));
        self
    }

    pub fn build(self) -> Result<BorgLLM> {
        if self.providers.is_empty() {
            return Err(LlmError::configuration(
                "at least one provider must be configured",
            ));
        }
        Ok(BorgLLM {
            providers: self.providers,
        })
    }
}

impl Default for BorgLLMBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::{ProviderBlock, StopReason};

    #[derive(Default)]
    struct FakeProvider {
        chat_results: Mutex<VecDeque<Result<LlmAssistantMessage>>>,
        transcription_results: Mutex<VecDeque<Result<String>>>,
    }

    impl FakeProvider {
        fn with_chat_results(results: Vec<Result<LlmAssistantMessage>>) -> Arc<Self> {
            Arc::new(Self {
                chat_results: Mutex::new(results.into()),
                transcription_results: Mutex::new(VecDeque::new()),
            })
        }

        fn with_transcription_results(results: Vec<Result<String>>) -> Arc<Self> {
            Arc::new(Self {
                chat_results: Mutex::new(VecDeque::new()),
                transcription_results: Mutex::new(results.into()),
            })
        }
    }

    #[async_trait]
    impl Provider for FakeProvider {
        async fn chat(&self, _req: &LlmRequest) -> Result<LlmAssistantMessage> {
            self.chat_results
                .lock()
                .expect("chat lock")
                .pop_front()
                .unwrap_or_else(|| Err(LlmError::message("no chat result queued")))
        }

        async fn transcribe(&self, _req: &TranscriptionRequest) -> Result<String> {
            self.transcription_results
                .lock()
                .expect("transcription lock")
                .pop_front()
                .unwrap_or_else(|| Err(LlmError::message("no transcription result queued")))
        }
    }

    struct NoAudioProvider(Arc<FakeProvider>);

    #[async_trait]
    impl Provider for NoAudioProvider {
        fn supports_audio_transcription(&self) -> bool {
            false
        }

        async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
            self.0.chat(req).await
        }

        async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
            self.0.transcribe(req).await
        }
    }

    fn chat_message(text: &str) -> LlmAssistantMessage {
        LlmAssistantMessage {
            content: vec![ProviderBlock::Text(text.to_string())],
            stop_reason: StopReason::EndOfTurn,
            error_message: None,
            usage_tokens: None,
        }
    }

    fn first_text(message: &LlmAssistantMessage) -> &str {
        match message.content.first() {
            Some(ProviderBlock::Text(text)) => text.as_str(),
            _ => panic!("expected first block to be text"),
        }
    }

    fn chat_request() -> LlmRequest {
        LlmRequest {
            model: "caller-model".to_string(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: Some(64),
            reasoning_effort: None,
            api_key: None,
        }
    }

    fn transcription_request(mime_type: &str) -> TranscriptionRequest {
        TranscriptionRequest {
            audio: vec![0x00],
            mime_type: mime_type.to_string(),
            model: None,
            language: None,
            prompt: None,
        }
    }

    #[tokio::test]
    async fn chat_uses_first_provider_when_it_succeeds() {
        let first = FakeProvider::with_chat_results(vec![Ok(chat_message("first"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("second"))]);
        let llm = BorgLLM::new(vec![first, second]);

        let chat = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat should succeed");

        assert_eq!(first_text(&chat), "first");
    }

    #[tokio::test]
    async fn chat_falls_back_to_next_provider_on_error() {
        let first =
            FakeProvider::with_chat_results(vec![Err(LlmError::message("402 payment required"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("fallback"))]);
        let llm = BorgLLM::new(vec![first, second]);

        let chat = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat should succeed");

        assert_eq!(first_text(&chat), "fallback");
    }

    #[tokio::test]
    async fn transcription_skips_providers_without_audio_support() {
        let first = NoAudioProvider(FakeProvider::with_transcription_results(vec![Err(
            LlmError::message("should not be called"),
        )]));
        let second = FakeProvider::with_transcription_results(vec![Ok("ok".to_string())]);
        let llm = BorgLLM::build()
            .add_provider(first)
            .add_provider(second)
            .build()
            .expect("llm should build");

        let text = llm
            .audio_transcription(&transcription_request("audio/ogg"))
            .await
            .expect("transcription should succeed");

        assert_eq!(text, "ok");
    }

    #[test]
    fn build_requires_at_least_one_provider() {
        let err = match BorgLLM::build().build() {
            Ok(_) => panic!("builder should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("at least one provider"));
    }
}
