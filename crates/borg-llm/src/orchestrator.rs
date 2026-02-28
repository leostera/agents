use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use anyhow::{Result, anyhow};

use crate::{LlmAssistantMessage, LlmRequest, Provider, TranscriptionRequest};

const DEFAULT_QUOTA_RESET_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    ChatCompletion,
    AudioTranscription,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsageKey {
    pub provider: String,
    pub capability: Capability,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct UsageState {
    pub used_tokens: u64,
    pub remaining_tokens: Option<u64>,
    pub reset_at: Option<SystemTime>,
}

pub trait UsageManager: Send + Sync {
    fn snapshot(&self, key: &UsageKey) -> UsageState;
    fn can_execute(&self, key: &UsageKey, now: SystemTime, estimated_tokens: Option<u64>) -> bool;
    fn record_success(
        &self,
        key: &UsageKey,
        used_tokens: u64,
        remaining_tokens: Option<u64>,
        reset_at: Option<SystemTime>,
    );
    fn mark_exhausted(&self, key: &UsageKey, reset_at: Option<SystemTime>);
}

#[derive(Default)]
pub struct InMemoryUsageManager {
    states: Mutex<HashMap<UsageKey, UsageState>>,
}

impl InMemoryUsageManager {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn insert_for_test(&self, key: UsageKey, state: UsageState) {
        self.states.lock().expect("usage lock").insert(key, state);
    }
}

impl UsageManager for InMemoryUsageManager {
    fn snapshot(&self, key: &UsageKey) -> UsageState {
        self.states
            .lock()
            .expect("usage lock")
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    fn can_execute(&self, key: &UsageKey, now: SystemTime, estimated_tokens: Option<u64>) -> bool {
        let state = self.snapshot(key);
        if let Some(reset_at) = state.reset_at
            && now >= reset_at
        {
            return true;
        }
        match (state.remaining_tokens, estimated_tokens) {
            (Some(remaining), Some(estimate)) => remaining >= estimate,
            (Some(remaining), None) => remaining > 0,
            (None, _) => true,
        }
    }

    fn record_success(
        &self,
        key: &UsageKey,
        used_tokens: u64,
        remaining_tokens: Option<u64>,
        reset_at: Option<SystemTime>,
    ) {
        self.states.lock().expect("usage lock").insert(
            key.clone(),
            UsageState {
                used_tokens,
                remaining_tokens,
                reset_at,
            },
        );
    }

    fn mark_exhausted(&self, key: &UsageKey, reset_at: Option<SystemTime>) {
        self.states.lock().expect("usage lock").insert(
            key.clone(),
            UsageState {
                used_tokens: 0,
                remaining_tokens: Some(0),
                reset_at,
            },
        );
    }
}

#[derive(Debug, Clone)]
pub struct ChatCompletionConfig {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AudioTranscriptionConfig {
    pub model: Option<String>,
    pub supported_formats: Vec<String>,
}

#[derive(Clone)]
pub struct ProviderRoute {
    pub provider_name: String,
    pub provider: Arc<dyn Provider>,
    pub chat_completion: Option<ChatCompletionConfig>,
    pub audio_transcription: Option<AudioTranscriptionConfig>,
}

impl ProviderRoute {
    pub fn new<P>(provider: P) -> Self
    where
        P: Provider + 'static,
    {
        let provider_name = provider.provider_name().to_string();
        let chat_completion = if provider.supports_chat_completion() {
            Some(ChatCompletionConfig {
                model: None,
                max_tokens: None,
            })
        } else {
            None
        };
        let audio_transcription = if provider.supports_audio_transcription() {
            Some(AudioTranscriptionConfig {
                model: None,
                supported_formats: vec![],
            })
        } else {
            None
        };

        Self {
            provider_name,
            provider: Arc::new(provider),
            chat_completion,
            audio_transcription,
        }
    }

    pub fn named<P>(provider_name: impl Into<String>, provider: P) -> Self
    where
        P: Provider + 'static,
    {
        let mut route = Self::new(provider);
        route.provider_name = provider_name.into();
        route
    }

    pub fn chat_key(&self, req: &LlmRequest) -> UsageKey {
        let model = self
            .chat_completion
            .as_ref()
            .and_then(|cfg| cfg.model.clone())
            .unwrap_or_else(|| req.model.clone());
        UsageKey {
            provider: self.provider_name.clone(),
            capability: Capability::ChatCompletion,
            model,
        }
    }

    pub fn transcription_key(&self, req: &TranscriptionRequest) -> UsageKey {
        let model = self
            .audio_transcription
            .as_ref()
            .and_then(|cfg| cfg.model.clone())
            .or_else(|| req.model.clone())
            .unwrap_or_else(|| "default".to_string());
        UsageKey {
            provider: self.provider_name.clone(),
            capability: Capability::AudioTranscription,
            model,
        }
    }
}

#[derive(Clone)]
#[allow(clippy::upper_case_acronyms)]
pub struct BorgLLM {
    providers: Vec<ProviderRoute>,
    usage_manager: Arc<dyn UsageManager>,
    quota_reset_window: Duration,
}

impl BorgLLM {
    pub fn build() -> BorgLLMBuilder {
        BorgLLMBuilder::new()
    }

    pub fn new(providers: Vec<ProviderRoute>, usage_manager: Arc<dyn UsageManager>) -> Self {
        Self {
            providers,
            usage_manager,
            quota_reset_window: DEFAULT_QUOTA_RESET_WINDOW,
        }
    }

    pub fn with_quota_reset_window(mut self, quota_reset_window: Duration) -> Self {
        self.quota_reset_window = quota_reset_window;
        self
    }

    pub async fn chat_completion(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let now = SystemTime::now();
        let estimated_tokens = req.max_tokens.map(u64::from);
        let mut last_error: Option<anyhow::Error> = None;

        for route in &self.providers {
            let Some(chat_cfg) = route.chat_completion.as_ref() else {
                continue;
            };

            let key = route.chat_key(req);
            if !self.usage_manager.can_execute(&key, now, estimated_tokens) {
                continue;
            }

            let mut routed = req.clone();
            if let Some(model) = &chat_cfg.model {
                routed.model = model.clone();
            }
            if routed.max_tokens.is_none() {
                routed.max_tokens = chat_cfg.max_tokens;
            }

            let result = route.provider.chat(&routed).await;
            match result {
                Ok(message) => {
                    self.usage_manager.record_success(&key, 0, None, None);
                    return Ok(message);
                }
                Err(error) if is_quota_error(&error) => {
                    self.usage_manager
                        .mark_exhausted(&key, now.checked_add(self.quota_reset_window));
                    last_error = Some(error);
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow!("no configured provider could satisfy chat completion")))
    }

    pub async fn audio_transcription(&self, req: &TranscriptionRequest) -> Result<String> {
        let now = SystemTime::now();
        let mut last_error: Option<anyhow::Error> = None;

        for route in &self.providers {
            let Some(cfg) = route.audio_transcription.as_ref() else {
                continue;
            };
            if !cfg.supported_formats.is_empty()
                && !cfg.supported_formats.iter().any(|f| f == &req.mime_type)
            {
                continue;
            }

            let key = route.transcription_key(req);
            if !self.usage_manager.can_execute(&key, now, None) {
                continue;
            }

            let mut routed = req.clone();
            if routed.model.is_none() {
                routed.model = cfg.model.clone();
            }

            let result = route.provider.transcribe(&routed).await;
            match result {
                Ok(text) => {
                    self.usage_manager.record_success(&key, 0, None, None);
                    return Ok(text);
                }
                Err(error) if is_quota_error(&error) => {
                    self.usage_manager
                        .mark_exhausted(&key, now.checked_add(self.quota_reset_window));
                    last_error = Some(error);
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow!("no configured provider could satisfy audio transcription")))
    }
}

pub struct BorgLLMBuilder {
    providers: Vec<ProviderRoute>,
    usage_manager: Arc<dyn UsageManager>,
    quota_reset_window: Duration,
}

impl BorgLLMBuilder {
    pub fn new() -> Self {
        Self {
            providers: vec![],
            usage_manager: Arc::new(InMemoryUsageManager::new()),
            quota_reset_window: DEFAULT_QUOTA_RESET_WINDOW,
        }
    }

    pub fn add_provider<P>(mut self, provider: P) -> Self
    where
        P: Into<ProviderRoute>,
    {
        self.providers.push(provider.into());
        self
    }

    pub fn with_usage_manager(mut self, usage_manager: Arc<dyn UsageManager>) -> Self {
        self.usage_manager = usage_manager;
        self
    }

    pub fn with_quota_reset_window(mut self, quota_reset_window: Duration) -> Self {
        self.quota_reset_window = quota_reset_window;
        self
    }

    pub fn build(self) -> Result<BorgLLM> {
        if self.providers.is_empty() {
            return Err(anyhow!("at least one provider must be configured"));
        }
        Ok(BorgLLM {
            providers: self.providers,
            usage_manager: self.usage_manager,
            quota_reset_window: self.quota_reset_window,
        })
    }
}

impl Default for BorgLLMBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> From<P> for ProviderRoute
where
    P: Provider + 'static,
{
    fn from(provider: P) -> Self {
        ProviderRoute::new(provider)
    }
}

fn is_quota_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("quota")
        || message.contains("rate limit")
        || message.contains("429")
        || message.contains("insufficient credits")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::{ProviderBlock, StopReason};

    #[derive(Default)]
    struct FakeProvider {
        chat_results: Mutex<VecDeque<Result<LlmAssistantMessage>>>,
        transcription_results: Mutex<VecDeque<Result<String>>>,
    }

    struct DenyAllUsageManager;

    impl UsageManager for DenyAllUsageManager {
        fn snapshot(&self, _key: &UsageKey) -> UsageState {
            UsageState::default()
        }

        fn can_execute(
            &self,
            _key: &UsageKey,
            _now: SystemTime,
            _estimated_tokens: Option<u64>,
        ) -> bool {
            false
        }

        fn record_success(
            &self,
            _key: &UsageKey,
            _used_tokens: u64,
            _remaining_tokens: Option<u64>,
            _reset_at: Option<SystemTime>,
        ) {
        }

        fn mark_exhausted(&self, _key: &UsageKey, _reset_at: Option<SystemTime>) {}
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
                .unwrap_or_else(|| Err(anyhow!("no chat result queued")))
        }

        async fn transcribe(&self, _req: &TranscriptionRequest) -> Result<String> {
            self.transcription_results
                .lock()
                .expect("transcription lock")
                .pop_front()
                .unwrap_or_else(|| Err(anyhow!("no transcription result queued")))
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
    async fn chat_uses_first_provider_that_supports_chat() {
        let first = FakeProvider::with_chat_results(vec![Ok(chat_message("first"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("second"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "p1".to_string(),
                    provider: first,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-1".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
                ProviderRoute {
                    provider_name: "p2".to_string(),
                    provider: second,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-2".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
            ],
            usage,
        );

        let message = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat result");
        assert_eq!(first_text(&message), "first");
    }

    #[test]
    fn builder_requires_at_least_one_provider() {
        let error = match BorgLLM::build().build() {
            Ok(_) => panic!("builder should fail with no providers"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("at least one provider must be configured")
        );
    }

    #[tokio::test]
    async fn builder_accepts_custom_usage_manager() {
        let provider = FakeProvider::with_chat_results(vec![Ok(chat_message("first"))]);
        let llm = BorgLLM::build()
            .add_provider(ProviderRoute {
                provider_name: "custom".to_string(),
                provider,
                chat_completion: Some(ChatCompletionConfig {
                    model: Some("model-1".to_string()),
                    max_tokens: None,
                }),
                audio_transcription: None,
            })
            .with_usage_manager(Arc::new(DenyAllUsageManager))
            .build()
            .expect("build llm");

        let error = llm
            .chat_completion(&chat_request())
            .await
            .expect_err("should fail");
        assert!(
            error
                .to_string()
                .contains("no configured provider could satisfy chat completion")
        );
    }

    #[tokio::test]
    async fn chat_falls_back_when_first_route_is_exhausted() {
        let first = FakeProvider::with_chat_results(vec![Ok(chat_message("first"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("second"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        usage.insert_for_test(
            UsageKey {
                provider: "p1".to_string(),
                capability: Capability::ChatCompletion,
                model: "model-1".to_string(),
            },
            UsageState {
                used_tokens: 0,
                remaining_tokens: Some(0),
                reset_at: None,
            },
        );
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "p1".to_string(),
                    provider: first,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-1".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
                ProviderRoute {
                    provider_name: "p2".to_string(),
                    provider: second,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-2".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
            ],
            usage,
        );

        let message = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat result");
        assert_eq!(first_text(&message), "second");
    }

    #[tokio::test]
    async fn chat_marks_route_exhausted_on_quota_error_and_falls_back() {
        let first = FakeProvider::with_chat_results(vec![Err(anyhow!("429 quota exceeded"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("second"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "p1".to_string(),
                    provider: first,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-1".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
                ProviderRoute {
                    provider_name: "p2".to_string(),
                    provider: second,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("model-2".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
            ],
            usage.clone(),
        );

        let message = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat result");
        assert_eq!(first_text(&message), "second");

        let state = usage.snapshot(&UsageKey {
            provider: "p1".to_string(),
            capability: Capability::ChatCompletion,
            model: "model-1".to_string(),
        });
        assert_eq!(state.remaining_tokens, Some(0));
        assert!(state.reset_at.is_some());
    }

    #[tokio::test]
    async fn transcription_uses_supported_route() {
        let first = FakeProvider::with_transcription_results(vec![Ok("first".to_string())]);
        let second = FakeProvider::with_transcription_results(vec![Ok("second".to_string())]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "p1".to_string(),
                    provider: first,
                    chat_completion: None,
                    audio_transcription: Some(AudioTranscriptionConfig {
                        model: Some("transcribe-1".to_string()),
                        supported_formats: vec!["audio/ogg".to_string()],
                    }),
                },
                ProviderRoute {
                    provider_name: "p2".to_string(),
                    provider: second,
                    chat_completion: None,
                    audio_transcription: Some(AudioTranscriptionConfig {
                        model: Some("transcribe-2".to_string()),
                        supported_formats: vec!["audio/wav".to_string()],
                    }),
                },
            ],
            usage,
        );

        let text = llm
            .audio_transcription(&transcription_request("audio/wav"))
            .await
            .expect("transcription");
        assert_eq!(text, "second");
    }

    #[tokio::test]
    async fn mixed_providers_split_capabilities_route_correctly() {
        let chat_provider = FakeProvider::with_chat_results(vec![Ok(chat_message("chat-ok"))]);
        let transcription_provider =
            FakeProvider::with_transcription_results(vec![Ok("transcript-ok".to_string())]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "openai-chat".to_string(),
                    provider: chat_provider,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("gpt-5.3-codex".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
                ProviderRoute {
                    provider_name: "openrouter-audio".to_string(),
                    provider: transcription_provider,
                    chat_completion: None,
                    audio_transcription: Some(AudioTranscriptionConfig {
                        model: Some("openai/gpt-4o-mini-transcribe".to_string()),
                        supported_formats: vec!["audio/ogg".to_string()],
                    }),
                },
            ],
            usage,
        );

        let chat = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat result");
        assert_eq!(first_text(&chat), "chat-ok");

        let text = llm
            .audio_transcription(&transcription_request("audio/ogg"))
            .await
            .expect("transcription");
        assert_eq!(text, "transcript-ok");
    }

    #[tokio::test]
    async fn transcription_falls_back_on_quota_error() {
        let first =
            FakeProvider::with_transcription_results(vec![Err(anyhow!("429 quota exceeded"))]);
        let second = FakeProvider::with_transcription_results(vec![Ok("fallback-ok".to_string())]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "openrouter-primary".to_string(),
                    provider: first,
                    chat_completion: None,
                    audio_transcription: Some(AudioTranscriptionConfig {
                        model: Some("openai/gpt-4o-mini-transcribe".to_string()),
                        supported_formats: vec!["audio/ogg".to_string()],
                    }),
                },
                ProviderRoute {
                    provider_name: "openai-fallback".to_string(),
                    provider: second,
                    chat_completion: None,
                    audio_transcription: Some(AudioTranscriptionConfig {
                        model: Some("gpt-4o-mini-transcribe".to_string()),
                        supported_formats: vec!["audio/ogg".to_string()],
                    }),
                },
            ],
            usage.clone(),
        );

        let text = llm
            .audio_transcription(&transcription_request("audio/ogg"))
            .await
            .expect("transcription");
        assert_eq!(text, "fallback-ok");

        let state = usage.snapshot(&UsageKey {
            provider: "openrouter-primary".to_string(),
            capability: Capability::AudioTranscription,
            model: "openai/gpt-4o-mini-transcribe".to_string(),
        });
        assert_eq!(state.remaining_tokens, Some(0));
        assert!(state.reset_at.is_some());
    }

    #[tokio::test]
    async fn chat_falls_back_across_mixed_routes() {
        let first = FakeProvider::with_chat_results(vec![Err(anyhow!("429 rate limit"))]);
        let second = FakeProvider::with_chat_results(vec![Ok(chat_message("fallback-chat"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![
                ProviderRoute {
                    provider_name: "openrouter-primary".to_string(),
                    provider: first,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("moonshot/kimi-k2".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
                ProviderRoute {
                    provider_name: "openai-fallback".to_string(),
                    provider: second,
                    chat_completion: Some(ChatCompletionConfig {
                        model: Some("gpt-5.3-codex".to_string()),
                        max_tokens: None,
                    }),
                    audio_transcription: None,
                },
            ],
            usage,
        );

        let chat = llm
            .chat_completion(&chat_request())
            .await
            .expect("chat result");
        assert_eq!(first_text(&chat), "fallback-chat");
    }

    #[tokio::test]
    async fn errors_when_no_provider_supports_capability() {
        let provider = FakeProvider::with_chat_results(vec![Ok(chat_message("ignored"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![ProviderRoute {
                provider_name: "p1".to_string(),
                provider,
                chat_completion: Some(ChatCompletionConfig {
                    model: Some("model-1".to_string()),
                    max_tokens: None,
                }),
                audio_transcription: None,
            }],
            usage,
        );

        let error = llm
            .audio_transcription(&transcription_request("audio/ogg"))
            .await
            .expect_err("should fail");
        assert!(
            error
                .to_string()
                .contains("no configured provider could satisfy audio transcription")
        );
    }

    #[tokio::test]
    async fn chat_records_success_usage_state() {
        let provider = FakeProvider::with_chat_results(vec![Ok(chat_message("ok"))]);
        let usage = Arc::new(InMemoryUsageManager::new());
        let llm = BorgLLM::new(
            vec![ProviderRoute {
                provider_name: "p1".to_string(),
                provider,
                chat_completion: Some(ChatCompletionConfig {
                    model: Some("model-1".to_string()),
                    max_tokens: Some(128),
                }),
                audio_transcription: None,
            }],
            usage.clone(),
        );

        llm.chat_completion(&chat_request())
            .await
            .expect("chat result");
        let state = usage.snapshot(&UsageKey {
            provider: "p1".to_string(),
            capability: Capability::ChatCompletion,
            model: "model-1".to_string(),
        });
        assert_eq!(state.used_tokens, 0);
        assert_eq!(state.remaining_tokens, None);
        assert_eq!(state.reset_at, None);
    }

    #[test]
    fn usage_manager_blocks_when_estimate_exceeds_remaining() {
        let usage = InMemoryUsageManager::new();
        let key = UsageKey {
            provider: "p1".to_string(),
            capability: Capability::ChatCompletion,
            model: "model".to_string(),
        };
        usage.insert_for_test(
            key.clone(),
            UsageState {
                used_tokens: 42,
                remaining_tokens: Some(3),
                reset_at: None,
            },
        );
        assert!(!usage.can_execute(&key, SystemTime::now(), Some(4)));
        assert!(usage.can_execute(&key, SystemTime::now(), Some(2)));
    }

    #[test]
    fn usage_manager_allows_after_reset_time() {
        let usage = InMemoryUsageManager::new();
        let key = UsageKey {
            provider: "p1".to_string(),
            capability: Capability::ChatCompletion,
            model: "model".to_string(),
        };
        usage.insert_for_test(
            key.clone(),
            UsageState {
                used_tokens: 99,
                remaining_tokens: Some(0),
                reset_at: Some(SystemTime::now() - Duration::from_secs(1)),
            },
        );
        assert!(usage.can_execute(&key, SystemTime::now(), Some(1)));
    }

    #[test]
    fn usage_key_contains_provider_capability_and_model() {
        let route = ProviderRoute {
            provider_name: "openrouter".to_string(),
            provider: Arc::new(FakeProvider::default()),
            chat_completion: Some(ChatCompletionConfig {
                model: Some("openrouter/kimi".to_string()),
                max_tokens: None,
            }),
            audio_transcription: Some(AudioTranscriptionConfig {
                model: Some("openrouter/whisper".to_string()),
                supported_formats: vec!["audio/ogg".to_string()],
            }),
        };
        let chat_key = route.chat_key(&chat_request());
        assert_eq!(chat_key.provider, "openrouter");
        assert_eq!(chat_key.capability, Capability::ChatCompletion);
        assert_eq!(chat_key.model, "openrouter/kimi");

        let tx_key = route.transcription_key(&transcription_request("audio/ogg"));
        assert_eq!(tx_key.provider, "openrouter");
        assert_eq!(tx_key.capability, Capability::AudioTranscription);
        assert_eq!(tx_key.model, "openrouter/whisper");
    }

    #[test]
    fn quota_error_detection_catches_common_cases() {
        assert!(is_quota_error(&anyhow!("429 Too Many Requests")));
        assert!(is_quota_error(&anyhow!("rate limit exceeded")));
        assert!(is_quota_error(&anyhow!("insufficient credits")));
        assert!(!is_quota_error(&anyhow!(
            json!({"error":"bad request"}).to_string()
        )));
    }
}
