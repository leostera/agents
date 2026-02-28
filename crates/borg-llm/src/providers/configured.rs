use anyhow::{Result, anyhow};
use async_trait::async_trait;
use tracing::info;

use crate::{
    AuthProvider, DeviceCodeAuthConfig, LlmAssistantMessage, LlmRequest, Provider,
    TranscriptionRequest,
};

use super::openai::{OpenAiApiMode, OpenAiProvider};

const OPENAI_PROVIDER: &str = "openai";
const OPENROUTER_PROVIDER: &str = "openrouter";
const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";
const OPENROUTER_TRANSCRIPTION_FALLBACK_ERROR: &str =
    "OpenAI provider key is required for transcription when preferred provider is `openrouter`";

#[derive(Clone, Debug, Default)]
pub struct ProviderSettings {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_api_mode: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: Option<String>,
    pub preferred_provider: Option<String>,
}

#[derive(Clone)]
pub enum ConfiguredProvider {
    OpenAi(OpenAiProvider),
    OpenRouter {
        chat: OpenAiProvider,
        openai_transcription_fallback: Option<OpenAiProvider>,
    },
}

impl ConfiguredProvider {
    pub fn from_settings(settings: ProviderSettings) -> Result<Self> {
        let ProviderSettings {
            openai_api_key,
            openai_base_url,
            openai_api_mode,
            openrouter_api_key,
            openrouter_base_url,
            preferred_provider,
        } = settings;
        let preferred = std::env::var("BORG_LLM_PROVIDER")
            .ok()
            .or(preferred_provider)
            .unwrap_or_else(|| OPENAI_PROVIDER.to_string())
            .to_lowercase();

        match preferred.as_str() {
            OPENAI_PROVIDER => {
                let api_key =
                    normalize_required(openai_api_key, "OpenAI provider is not configured")?;
                let api_mode_raw = openai_api_mode
                    .or_else(|| std::env::var("BORG_OPENAI_API_MODE").ok())
                    .unwrap_or_else(|| "completions".to_string())
                    .to_lowercase();
                let api_mode = match api_mode_raw.as_str() {
                    "chat" | "chat_completions" => OpenAiApiMode::ChatCompletions,
                    "completions" => OpenAiApiMode::Completions,
                    _ => {
                        return Err(anyhow!(
                            "unsupported OpenAI API mode `{}` (expected `chat_completions` or `completions`)",
                            api_mode_raw
                        ));
                    }
                };
                let provider = build_openai_provider(api_key, openai_base_url, api_mode);

                info!(
                    target: "borg_llm",
                    provider = OPENAI_PROVIDER,
                    "configured provider selected"
                );
                Ok(Self::OpenAi(provider))
            }
            OPENROUTER_PROVIDER => {
                let openrouter_api_key = normalize_required(
                    openrouter_api_key,
                    "OpenRouter provider is not configured",
                )?;
                let openrouter_base_url = normalize_optional(openrouter_base_url)
                    .or_else(|| normalize_optional(std::env::var("BORG_OPENROUTER_BASE_URL").ok()))
                    .unwrap_or_else(|| OPENROUTER_DEFAULT_BASE_URL.to_string());
                let chat = OpenAiProvider::new_with_base_url_and_mode(
                    openrouter_api_key,
                    openrouter_base_url,
                    OpenAiApiMode::ChatCompletions,
                );

                let openai_transcription_fallback =
                    normalize_optional(openai_api_key).map(|openai_api_key| {
                        build_openai_provider(
                            openai_api_key,
                            openai_base_url.clone(),
                            OpenAiApiMode::ChatCompletions,
                        )
                    });

                info!(
                    target: "borg_llm",
                    provider = OPENROUTER_PROVIDER,
                    has_openai_transcription_fallback = openai_transcription_fallback.is_some(),
                    "configured provider selected"
                );
                Ok(Self::OpenRouter {
                    chat,
                    openai_transcription_fallback,
                })
            }
            _ => Err(anyhow!(
                "unsupported BORG_LLM_PROVIDER `{}` (expected `openai` or `openrouter`)",
                preferred
            )),
        }
    }
}

#[async_trait]
impl Provider for ConfiguredProvider {
    fn provider_name(&self) -> &'static str {
        match self {
            Self::OpenAi(_) => OPENAI_PROVIDER,
            Self::OpenRouter { .. } => OPENROUTER_PROVIDER,
        }
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        match self {
            Self::OpenAi(provider) => provider.chat(req).await,
            Self::OpenRouter { chat, .. } => chat.chat(req).await,
        }
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        match self {
            Self::OpenAi(provider) => provider.transcribe(req).await,
            Self::OpenRouter {
                openai_transcription_fallback: Some(fallback),
                ..
            } => fallback.transcribe(req).await,
            Self::OpenRouter { .. } => Err(anyhow!(OPENROUTER_TRANSCRIPTION_FALLBACK_ERROR)),
        }
    }
}

impl AuthProvider for ConfiguredProvider {
    fn device_code_auth_config(&self) -> Option<DeviceCodeAuthConfig> {
        match self {
            Self::OpenAi(provider) => provider.device_code_auth_config(),
            Self::OpenRouter { .. } => None,
        }
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
}

fn normalize_required(value: Option<String>, error_message: &str) -> Result<String> {
    normalize_optional(value).ok_or_else(|| anyhow!(error_message.to_string()))
}

fn build_openai_provider(
    api_key: String,
    base_url: Option<String>,
    api_mode: OpenAiApiMode,
) -> OpenAiProvider {
    if let Some(base_url) = normalize_optional(base_url) {
        OpenAiProvider::new_with_base_url_and_mode(api_key, base_url, api_mode)
    } else {
        OpenAiProvider::new_with_mode(api_key, api_mode)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;

    fn env_lock() -> MutexGuard<'static, ()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: Tests synchronize environment mutations with a global mutex.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_ref() {
                // SAFETY: Tests synchronize environment mutations with a global mutex.
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            } else {
                // SAFETY: Tests synchronize environment mutations with a global mutex.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn from_settings_selects_openrouter_when_preferred() {
        let _guard = env_lock();
        let provider = ConfiguredProvider::from_settings(ProviderSettings {
            openrouter_api_key: Some("or-key".to_string()),
            preferred_provider: Some("openrouter".to_string()),
            ..ProviderSettings::default()
        })
        .expect("configured provider");
        assert!(matches!(provider, ConfiguredProvider::OpenRouter { .. }));
    }

    #[test]
    fn env_override_wins_over_preferred_provider_setting() {
        let _guard = env_lock();
        let _provider_override = EnvVarGuard::set("BORG_LLM_PROVIDER", OPENAI_PROVIDER);
        let provider = ConfiguredProvider::from_settings(ProviderSettings {
            openai_api_key: Some("sk-test".to_string()),
            openrouter_api_key: Some("or-key".to_string()),
            preferred_provider: Some(OPENROUTER_PROVIDER.to_string()),
            ..ProviderSettings::default()
        })
        .expect("configured provider");
        assert!(matches!(provider, ConfiguredProvider::OpenAi(_)));
    }

    #[tokio::test]
    async fn openrouter_transcribe_without_openai_key_returns_clear_error() {
        let _guard = env_lock();
        let provider = ConfiguredProvider::from_settings(ProviderSettings {
            openrouter_api_key: Some("or-key".to_string()),
            preferred_provider: Some("openrouter".to_string()),
            ..ProviderSettings::default()
        })
        .expect("configured provider");

        let err = provider
            .transcribe(&TranscriptionRequest {
                audio: vec![0x00, 0x01],
                mime_type: "audio/ogg".to_string(),
                model: None,
                language: None,
                prompt: None,
            })
            .await
            .expect_err("transcription should fail");

        assert_eq!(err.to_string(), OPENROUTER_TRANSCRIPTION_FALLBACK_ERROR);
    }
}
