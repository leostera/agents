use anyhow::{Result, anyhow};
use async_trait::async_trait;
use tracing::info;

use crate::{LlmAssistantMessage, LlmRequest, Provider, TranscriptionRequest};

use super::openai::{OpenAiApiMode, OpenAiProvider};

const OPENAI_PROVIDER: &str = "openai";

#[derive(Clone, Debug, Default)]
pub struct ProviderSettings {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_api_mode: Option<String>,
    pub preferred_provider: Option<String>,
}

#[derive(Clone)]
pub enum ConfiguredProvider {
    OpenAi(OpenAiProvider),
}

impl ConfiguredProvider {
    pub fn from_settings(settings: ProviderSettings) -> Result<Self> {
        let preferred = settings
            .preferred_provider
            .or_else(|| std::env::var("BORG_LLM_PROVIDER").ok())
            .unwrap_or_else(|| OPENAI_PROVIDER.to_string())
            .to_lowercase();

        if preferred.as_str() != OPENAI_PROVIDER {
            return Err(anyhow!(
                "unsupported BORG_LLM_PROVIDER `{}` (expected `openai`)",
                preferred
            ));
        }

        let api_key = settings
            .openai_api_key
            .as_ref()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .ok_or_else(|| anyhow!("OpenAI provider is not configured"))?;

        let api_mode_raw = settings
            .openai_api_mode
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

        let provider = if let Some(base_url) = settings
            .openai_base_url
            .as_ref()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
        {
            OpenAiProvider::new_with_base_url_and_mode(api_key, base_url, api_mode)
        } else {
            OpenAiProvider::new_with_mode(api_key, api_mode)
        };

        info!(
            target: "borg_llm",
            provider = OPENAI_PROVIDER,
            "configured provider selected"
        );
        Ok(Self::OpenAi(provider))
    }
}

#[async_trait]
impl Provider for ConfiguredProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        match self {
            Self::OpenAi(provider) => provider.chat(req).await,
        }
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        match self {
            Self::OpenAi(provider) => provider.transcribe(req).await,
        }
    }
}
