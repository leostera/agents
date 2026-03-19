use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::llm::capability::Capability;
use crate::llm::completion::{
    ProviderType, RawCompletionEvent, RawCompletionEventStream, RawCompletionRequest,
    RawCompletionResponse,
};
use crate::llm::error::{Error, LlmResult, WorkersAIConfigError};
use crate::llm::model::Model;
use crate::llm::provider::LlmProvider;
use crate::llm::provider::openai::{OpenAI, OpenAIConfig};
use crate::llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct WorkersAIConfig {
    pub api_token: String,
    pub account_id: String,
    pub base_url: String,
    pub default_model: String,
}

impl WorkersAIConfig {
    pub fn new(
        api_token: impl Into<String>,
        account_id: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self, WorkersAIConfigError> {
        let api_token = api_token.into();
        if api_token.is_empty() {
            return Err(WorkersAIConfigError::MissingApiToken);
        }

        let account_id = account_id.into();
        if account_id.is_empty() {
            return Err(WorkersAIConfigError::MissingAccountId);
        }

        Ok(Self {
            api_token,
            base_url: default_base_url(&account_id),
            account_id,
            default_model: default_model.into(),
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn into_openai_config(self) -> OpenAIConfig {
        OpenAIConfig::new(self.api_token, self.default_model)
            .expect("workers ai config should already validate token")
            .with_base_url(self.base_url)
    }
}

pub struct WorkersAI {
    inner: OpenAI,
    config: WorkersAIConfig,
}

impl WorkersAI {
    pub fn new(config: WorkersAIConfig) -> Self {
        let inner = OpenAI::new(config.clone().into_openai_config());
        Self { inner, config }
    }
}

#[async_trait]
impl LlmProvider for WorkersAI {
    fn provider_type(&self) -> ProviderType {
        ProviderType::WorkersAI
    }

    fn provider_name(&self) -> &'static str {
        "workers_ai"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::ChatCompletion]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        Ok(vec![Model::new(self.config.default_model.clone())])
    }

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let mut response = self.inner.chat_raw(req).await.map_err(remap_error)?;
        response.provider = ProviderType::WorkersAI;
        Ok(response)
    }

    async fn chat_raw_stream(
        &self,
        req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        let mut inner_stream = self.inner.chat_raw_stream(req).await.map_err(remap_error)?;
        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Some(event) = inner_stream.recv().await {
                let mapped = event.map(remap_stream_event).map_err(remap_error);
                if sender.send(mapped).await.is_err() {
                    return;
                }
            }
        });

        Ok(RawCompletionEventStream::new(receiver))
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::InvalidRequest {
            reason: "Workers AI transcription is not supported by this provider yet".to_string(),
        })
    }
}

fn remap_stream_event(event: RawCompletionEvent) -> RawCompletionEvent {
    match event {
        RawCompletionEvent::Done(mut response) => {
            response.provider = ProviderType::WorkersAI;
            RawCompletionEvent::Done(response)
        }
        other => other,
    }
}

fn remap_error(error: Error) -> Error {
    match error {
        Error::Provider {
            provider,
            status,
            message,
        } => Error::Provider {
            provider: remap_provider_name(provider),
            status,
            message,
        },
        Error::RateLimited {
            provider,
            retry_after,
        } => Error::RateLimited {
            provider: remap_provider_name(provider),
            retry_after,
        },
        Error::Authentication { provider, message } => Error::Authentication {
            provider: remap_provider_name(provider),
            message,
        },
        other => other,
    }
}

fn remap_provider_name(provider: String) -> String {
    if provider == "openai" {
        "workers_ai".to_string()
    } else {
        provider
    }
}

fn default_base_url(account_id: &str) -> String {
    format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1")
}

#[cfg(test)]
mod tests {
    use super::{WorkersAI, WorkersAIConfig, default_base_url};
    use crate::llm::completion::ProviderType;
    use crate::llm::provider::LlmProvider;

    #[test]
    fn workers_ai_config_builds_default_base_url() {
        let config = WorkersAIConfig::new("token", "account", "@cf/meta/llama-3.1-8b-instruct")
            .expect("config");
        assert_eq!(config.base_url, default_base_url("account"));
    }

    #[test]
    fn workers_ai_reports_provider_identity() {
        let provider = WorkersAI::new(
            WorkersAIConfig::new("token", "account", "@cf/meta/llama-3.1-8b-instruct")
                .expect("config"),
        );
        assert_eq!(provider.provider_type(), ProviderType::WorkersAI);
        assert_eq!(provider.provider_name(), "workers_ai");
    }
}
