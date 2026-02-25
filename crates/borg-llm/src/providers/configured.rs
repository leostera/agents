use anyhow::{Result, anyhow};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::{LlmAssistantMessage, LlmRequest, Provider};

use super::kalosm::KalosmProvider;
use super::openai::OpenAiProvider;

const OPENAI_PROVIDER: &str = "openai";
const KALOSM_PROVIDER: &str = "kalosm";
const DEFAULT_KALOSM_MODEL: &str = "qwen2.5:0.5b";

#[derive(Clone, Debug, Default)]
pub struct ProviderSettings {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub preferred_provider: Option<String>,
    pub kalosm_model: Option<String>,
}

#[derive(Clone)]
pub enum ConfiguredProvider {
    OpenAi(OpenAiProvider),
    Kalosm(KalosmProvider),
}

impl ConfiguredProvider {
    pub fn from_settings(settings: ProviderSettings) -> Result<Self> {
        let preferred = settings
            .preferred_provider
            .or_else(|| std::env::var("BORG_LLM_PROVIDER").ok())
            .unwrap_or_else(|| OPENAI_PROVIDER.to_string())
            .to_lowercase();

        let kalosm_model = settings
            .kalosm_model
            .or_else(|| std::env::var("BORG_KALOSM_MODEL").ok())
            .unwrap_or_else(|| DEFAULT_KALOSM_MODEL.to_string());

        match preferred.as_str() {
            OPENAI_PROVIDER => {
                if let Some(api_key) = settings
                    .openai_api_key
                    .as_ref()
                    .map(|key| key.trim().to_string())
                    .filter(|key| !key.is_empty())
                {
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
                } else {
                    warn!(
                        target: "borg_llm",
                        requested_provider = OPENAI_PROVIDER,
                        fallback_provider = KALOSM_PROVIDER,
                        model = kalosm_model.as_str(),
                        "openai key missing; falling back to kalosm provider"
                    );
                    Ok(Self::Kalosm(KalosmProvider::new(kalosm_model)))
                }
            }
            KALOSM_PROVIDER => {
                info!(
                    target: "borg_llm",
                    provider = KALOSM_PROVIDER,
                    model = kalosm_model.as_str(),
                    "configured provider selected"
                );
                Ok(Self::Kalosm(KalosmProvider::new(kalosm_model)))
            }
            other => Err(anyhow!(
                "unsupported BORG_LLM_PROVIDER `{}` (expected `openai` or `kalosm`)",
                other
            )),
        }
    }
}

#[async_trait]
impl Provider for ConfiguredProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        match self {
            Self::OpenAi(provider) => provider.chat(req).await,
            Self::Kalosm(provider) => provider.chat(req).await,
        }
    }
}
