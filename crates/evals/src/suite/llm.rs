use std::env;

use agents::llm::LlmRunner;
use agents::llm::error::Error as LlmError;
use agents::llm::provider::anthropic::{Anthropic, AnthropicConfig};
use agents::llm::provider::apple::{Apple, AppleConfig};
use agents::llm::provider::lm_studio::{LmStudio, LmStudioConfig};
use agents::llm::provider::ollama::{Ollama, OllamaConfig};
use agents::llm::provider::openai::{OpenAI, OpenAIConfig};
use agents::llm::provider::openrouter::{OpenRouter, OpenRouterConfig};
use agents::llm::provider::workers_ai::{WorkersAI, WorkersAIConfig};

use super::*;

pub(super) fn llm_runner_for_target(
    target: &ExecutionTarget,
    provider_configs: &ProviderConfigs,
) -> EvalResult<LlmRunner> {
    let runner = match target.provider.as_str() {
        "default" => LlmRunner::builder().build(),
        "ollama" => {
            let mut config = OllamaConfig::new(target.model.clone());
            if let Some(base_url) = optional_env(&["BORG_LLM_OLLAMA_BASE_URL", "OLLAMA_BASE_URL"])
                .or_else(|| {
                    provider_configs
                        .ollama
                        .as_ref()
                        .map(|config| config.url.clone())
                })
            {
                config = config.with_base_url(base_url);
            }
            LlmRunner::builder()
                .add_provider(Ollama::new(config))
                .build()
        }
        "lm_studio" => {
            let mut config = LmStudioConfig::new(target.model.clone());
            if let Some(base_url) =
                optional_env(&["BORG_LLM_LM_STUDIO_BASE_URL", "LM_STUDIO_BASE_URL"]).or_else(|| {
                    provider_configs
                        .lm_studio
                        .as_ref()
                        .and_then(|config| config.url.clone())
                })
            {
                config = config.with_base_url(base_url);
            }
            if let Some(token) =
                optional_env(&["BORG_LLM_LM_STUDIO_API_TOKEN", "LM_STUDIO_API_TOKEN"]).or_else(
                    || {
                        provider_configs
                            .lm_studio
                            .as_ref()
                            .and_then(|config| config.api_token.clone())
                    },
                )
            {
                config = config.with_api_token(token);
            }
            LlmRunner::builder()
                .add_provider(LmStudio::new(config))
                .build()
        }
        "openai" => {
            let Some(api_key) = optional_env(&["BORG_LLM_OPENAI_API_KEY", "OPENAI_API_KEY"])
                .or_else(|| {
                    provider_configs
                        .openai
                        .as_ref()
                        .and_then(|config| config.api_key.clone())
                })
            else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = OpenAIConfig::new(api_key, target.model.clone())
                .map_err(LlmError::OpenAIConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_OPENAI_BASE_URL"]).or_else(|| {
                provider_configs
                    .openai
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            if let Some(org) = optional_env(&["BORG_LLM_OPENAI_ORGANIZATION", "OPENAI_ORG_ID"])
                .or_else(|| {
                    provider_configs
                        .openai
                        .as_ref()
                        .and_then(|config| config.organization.clone())
                })
            {
                config = config.with_organization(org);
            }
            LlmRunner::builder()
                .add_provider(OpenAI::new(config))
                .build()
        }
        "anthropic" => {
            let Some(api_key) = optional_env(&["BORG_LLM_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"])
                .or_else(|| {
                    provider_configs
                        .anthropic
                        .as_ref()
                        .and_then(|config| config.api_key.clone())
                })
            else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = AnthropicConfig::new(api_key, target.model.clone())
                .map_err(LlmError::AnthropicConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_ANTHROPIC_BASE_URL"]).or_else(|| {
                provider_configs
                    .anthropic
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            if let Some(version) = provider_configs
                .anthropic
                .as_ref()
                .and_then(|config| config.version.clone())
            {
                config = config.with_version(version);
            }
            LlmRunner::builder()
                .add_provider(Anthropic::new(config))
                .build()
        }
        "openrouter" => {
            let Some(api_key) =
                optional_env(&["BORG_LLM_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"]).or_else(
                    || {
                        provider_configs
                            .openrouter
                            .as_ref()
                            .and_then(|config| config.api_key.clone())
                    },
                )
            else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = OpenRouterConfig::new(api_key, target.model.clone())
                .map_err(LlmError::OpenRouterConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_OPENROUTER_BASE_URL"]).or_else(|| {
                provider_configs
                    .openrouter
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            LlmRunner::builder()
                .add_provider(OpenRouter::new(config))
                .build()
        }
        "workers_ai" => {
            let Some(api_token) =
                optional_env(&["BORG_LLM_WORKERS_AI_API_TOKEN", "CLOUDFLARE_API_TOKEN"]).or_else(
                    || {
                        provider_configs
                            .workers_ai
                            .as_ref()
                            .and_then(|config| config.api_token.clone())
                    },
                )
            else {
                return Ok(LlmRunner::builder().build());
            };
            let Some(account_id) = optional_env(&["CLOUDFLARE_ACCOUNT_ID"]).or_else(|| {
                provider_configs
                    .workers_ai
                    .as_ref()
                    .and_then(|config| config.account_id.clone())
            }) else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = WorkersAIConfig::new(api_token, account_id, target.model.clone())
                .map_err(LlmError::WorkersAIConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_WORKERS_AI_BASE_URL"]).or_else(|| {
                provider_configs
                    .workers_ai
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            LlmRunner::builder()
                .add_provider(WorkersAI::new(config))
                .build()
        }
        "apple" => LlmRunner::builder()
            .add_provider(Apple::new(AppleConfig::new()))
            .build(),
        provider => {
            return Err(EvalError::message(format!(
                "unsupported eval target provider {:?}",
                provider
            )));
        }
    };

    Ok(runner)
}

fn optional_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
}
