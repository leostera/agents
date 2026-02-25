use anyhow::{Result, anyhow};
use async_trait::async_trait;
use tracing::info;

use crate::{LlmAssistantMessage, LlmRequest, Provider};

use super::openai::OpenAiProvider;

const OPENAI_PROVIDER: &str = "openai";

#[derive(Clone, Debug, Default)]
pub struct ProviderSettings {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
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

        let provider = if let Some(base_url) = settings
            .openai_base_url
            .as_ref()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
        {
            OpenAiProvider::new_with_base_url(api_key, base_url)
        } else {
            OpenAiProvider::new(api_key)
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
}
