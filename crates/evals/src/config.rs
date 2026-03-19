use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Provider-specific overrides loaded from `evals.toml` or built in code.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProviderConfigs {
    pub ollama: Option<OllamaProviderConfig>,
    pub openai: Option<OpenAIProviderConfig>,
    pub anthropic: Option<AnthropicProviderConfig>,
    pub openrouter: Option<OpenRouterProviderConfig>,
    pub lm_studio: Option<LmStudioProviderConfig>,
}

/// Runtime override for Ollama targets.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OllamaProviderConfig {
    pub url: String,
}

/// Runtime override for OpenAI targets.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OpenAIProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub organization: Option<String>,
}

/// Runtime override for Anthropic targets.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnthropicProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub version: Option<String>,
}

/// Runtime override for OpenRouter targets.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OpenRouterProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// Runtime override for LM Studio targets.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LmStudioProviderConfig {
    pub url: Option<String>,
    pub api_token: Option<String>,
}

/// One concrete model target to evaluate against.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExecutionTarget {
    pub label: String,
    pub provider: String,
    pub model: String,
    pub max_in_flight: usize,
}

impl ExecutionTarget {
    pub fn new(
        label: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let provider = provider.into();
        Self {
            label: label.into(),
            max_in_flight: default_max_in_flight(&provider),
            provider,
            model: model.into(),
        }
    }

    pub fn ollama(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "ollama", model)
    }

    pub fn openai(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "openai", model)
    }

    pub fn anthropic(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "anthropic", model)
    }

    pub fn openrouter(label: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(label, "openrouter", model)
    }

    pub fn with_max_in_flight(mut self, max_in_flight: usize) -> Self {
        self.max_in_flight = max_in_flight.max(1);
        self
    }

    pub fn display_label(&self) -> String {
        self.label.clone()
    }

    pub fn is_local(&self) -> bool {
        is_local_provider(&self.provider)
    }
}

impl Default for ExecutionTarget {
    fn default() -> Self {
        Self::new("default", "default", "default")
    }
}

/// Top-level configuration for an eval run.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunConfig {
    pub targets: Vec<ExecutionTarget>,
    pub trials: usize,
    pub timeout: Option<Duration>,
    pub provider: ProviderConfigs,
}

impl RunConfig {
    pub fn new(targets: Vec<ExecutionTarget>) -> Self {
        Self {
            targets,
            trials: 1,
            timeout: None,
            provider: ProviderConfigs::default(),
        }
    }

    pub fn single(target: ExecutionTarget) -> Self {
        Self::new(vec![target])
    }

    pub fn with_trials(mut self, trials: usize) -> Self {
        self.trials = trials.max(1);
        self
    }

    pub fn with_provider_configs(mut self, provider: ProviderConfigs) -> Self {
        self.provider = provider;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_optional_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

fn default_max_in_flight(provider: &str) -> usize {
    if is_local_provider(provider) { 1 } else { 5 }
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "lm_studio" | "apple")
}

impl Default for RunConfig {
    fn default() -> Self {
        Self::single(ExecutionTarget::default())
    }
}
