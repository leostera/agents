use std::path::Path;
use std::time::Duration;

use crate::{
    AnthropicProviderConfig as PublicAnthropicProviderConfig, ExecutionTarget,
    LmStudioProviderConfig as PublicLmStudioProviderConfig,
    OllamaProviderConfig as PublicOllamaProviderConfig,
    OpenAIProviderConfig as PublicOpenAIProviderConfig,
    OpenRouterProviderConfig as PublicOpenRouterProviderConfig, ProviderConfigs, RunConfig,
    WorkersAIProviderConfig as PublicWorkersAIProviderConfig,
};
use agents::llm::completion::ProviderType;
use anyhow::{Context, Result, bail};
use config::{Config, File, FileFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct EvalsFile {
    pub evals: EvalsConfig,
    #[serde(default)]
    pub provider: ProviderConfigSet,
}

#[derive(Debug, Deserialize)]
pub(super) struct EvalsConfig {
    #[serde(default = "default_trials")]
    pub trials: usize,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    pub timeout: Option<DurationValue>,
    pub timeout_secs: Option<u64>,
    #[serde(skip)]
    pub resolved_timeout: Option<Duration>,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum DurationValue {
    Human(String),
    Seconds(u64),
}

#[derive(Debug, Deserialize)]
pub(super) struct TargetConfig {
    pub label: Option<String>,
    pub provider: String,
    pub model: String,
    pub concurrency: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ProviderConfigSet {
    pub ollama: Option<OllamaProviderConfig>,
    pub openai: Option<OpenAIProviderConfig>,
    pub anthropic: Option<AnthropicProviderConfig>,
    pub openrouter: Option<OpenRouterProviderConfig>,
    pub workers_ai: Option<WorkersAIProviderConfig>,
    pub lm_studio: Option<LmStudioProviderConfig>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OllamaProviderConfig {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAIProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub organization: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenRouterProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WorkersAIProviderConfig {
    pub api_token: Option<String>,
    pub account_id: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LmStudioProviderConfig {
    pub url: Option<String>,
    pub api_token: Option<String>,
}

impl EvalsFile {
    pub(super) fn load(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root.join("evals.toml");
        let mut file: EvalsFile = Config::builder()
            .add_source(File::from(path.clone()).format(FileFormat::Toml))
            .build()
            .with_context(|| format!("load {}", path.display()))?
            .try_deserialize()
            .with_context(|| format!("parse {}", path.display()))?;
        file.validate()?;
        Ok(file)
    }

    pub(super) fn run_config(&self) -> RunConfig {
        RunConfig::new(
            self.evals
                .targets
                .iter()
                .map(|target| {
                    ExecutionTarget::new(
                        target.label.as_deref().unwrap_or(""),
                        &target.provider,
                        &target.model,
                    )
                    .with_max_in_flight(target.concurrency.unwrap_or(1))
                })
                .collect(),
        )
        .with_trials(self.evals.trials)
        .with_provider_configs(self.provider_configs())
        .with_optional_timeout(self.evals.resolved_timeout)
    }

    pub(super) fn output_dir(&self) -> &str {
        &self.evals.output_dir
    }

    fn provider_configs(&self) -> ProviderConfigs {
        ProviderConfigs {
            ollama: self
                .provider
                .ollama
                .as_ref()
                .map(|config| PublicOllamaProviderConfig {
                    url: config.url.clone(),
                }),
            openai: self
                .provider
                .openai
                .as_ref()
                .map(|config| PublicOpenAIProviderConfig {
                    api_key: config.api_key.clone(),
                    base_url: config.base_url.clone(),
                    organization: config.organization.clone(),
                }),
            anthropic: self.provider.anthropic.as_ref().map(|config| {
                PublicAnthropicProviderConfig {
                    api_key: config.api_key.clone(),
                    base_url: config.base_url.clone(),
                    version: config.version.clone(),
                }
            }),
            openrouter: self.provider.openrouter.as_ref().map(|config| {
                PublicOpenRouterProviderConfig {
                    api_key: config.api_key.clone(),
                    base_url: config.base_url.clone(),
                }
            }),
            workers_ai: self.provider.workers_ai.as_ref().map(|config| {
                PublicWorkersAIProviderConfig {
                    api_token: config.api_token.clone(),
                    account_id: config.account_id.clone(),
                    base_url: config.base_url.clone(),
                }
            }),
            lm_studio: self.provider.lm_studio.as_ref().map(|config| {
                PublicLmStudioProviderConfig {
                    url: config.url.clone(),
                    api_token: config.api_token.clone(),
                }
            }),
        }
    }

    fn validate(&mut self) -> Result<()> {
        self.evals.resolved_timeout =
            resolve_timeout(&self.evals.timeout, self.evals.timeout_secs)?;
        for target in &mut self.evals.targets {
            target.validate()?;
        }
        if let Some(ollama) = &mut self.provider.ollama {
            ollama.url = ollama.url.trim().to_string();
            if ollama.url.is_empty() {
                bail!("provider.ollama.url cannot be empty");
            }
        }
        if let Some(openai) = &mut self.provider.openai {
            trim_optional_string(&mut openai.api_key);
            trim_optional_string(&mut openai.base_url);
            trim_optional_string(&mut openai.organization);
        }
        if let Some(anthropic) = &mut self.provider.anthropic {
            trim_optional_string(&mut anthropic.api_key);
            trim_optional_string(&mut anthropic.base_url);
            trim_optional_string(&mut anthropic.version);
        }
        if let Some(openrouter) = &mut self.provider.openrouter {
            trim_optional_string(&mut openrouter.api_key);
            trim_optional_string(&mut openrouter.base_url);
        }
        if let Some(workers_ai) = &mut self.provider.workers_ai {
            trim_optional_string(&mut workers_ai.api_token);
            trim_optional_string(&mut workers_ai.account_id);
            trim_optional_string(&mut workers_ai.base_url);
        }
        if let Some(lm_studio) = &mut self.provider.lm_studio {
            trim_optional_string(&mut lm_studio.url);
            trim_optional_string(&mut lm_studio.api_token);
        }
        Ok(())
    }
}

fn trim_optional_string(value: &mut Option<String>) {
    if let Some(value_ref) = value.as_mut() {
        *value_ref = value_ref.trim().to_string();
    }
}

impl TargetConfig {
    fn validate(&mut self) -> Result<()> {
        let provider = self.provider.trim();
        if !supported_provider(provider) {
            bail!(
                "unsupported eval target provider {:?}; expected one of: openai, anthropic, openrouter, workers_ai, lm_studio, ollama, apple",
                self.provider
            );
        }

        let model = self.model.trim();
        if model.is_empty() {
            bail!(
                "eval target model cannot be empty for provider {:?}",
                self.provider
            );
        }

        self.provider = provider.to_string();
        self.model = model.to_string();
        self.label = Some(match &self.label {
            Some(label) if !label.trim().is_empty() => label.trim().to_string(),
            _ => format!("{}/{}", self.provider, self.model),
        });

        Ok(())
    }
}

fn supported_provider(provider: &str) -> bool {
    [
        ProviderType::OpenAI,
        ProviderType::Anthropic,
        ProviderType::OpenRouter,
        ProviderType::WorkersAI,
        ProviderType::LmStudio,
        ProviderType::Ollama,
        ProviderType::Apple,
    ]
    .iter()
    .any(|supported| supported.name() == provider)
}

fn default_trials() -> usize {
    10
}

fn default_output_dir() -> String {
    ".evals".to_string()
}

fn resolve_timeout(
    timeout: &Option<DurationValue>,
    timeout_secs: Option<u64>,
) -> Result<Option<Duration>> {
    if timeout.is_some() && timeout_secs.is_some() {
        bail!("use evals.timeout instead of combining it with evals.timeout_secs");
    }

    match (timeout, timeout_secs) {
        (Some(timeout), None) => parse_duration_value("evals.timeout", timeout).map(Some),
        (None, Some(timeout_secs)) => {
            if timeout_secs == 0 {
                bail!("evals.timeout_secs must be greater than zero");
            }
            Ok(Some(Duration::from_secs(timeout_secs)))
        }
        (None, None) => Ok(None),
        (Some(_), Some(_)) => unreachable!("handled above"),
    }
}

fn parse_duration_value(field_name: &str, value: &DurationValue) -> Result<Duration> {
    match value {
        DurationValue::Human(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                bail!("{field_name} cannot be empty");
            }
            let duration = humantime::parse_duration(trimmed).with_context(|| {
                format!("parse {field_name} as a duration like \"30s\" or \"2m\"")
            })?;
            if duration.is_zero() {
                bail!("{field_name} must be greater than zero");
            }
            Ok(duration)
        }
        DurationValue::Seconds(seconds) => {
            if *seconds == 0 {
                bail!("{field_name} must be greater than zero");
            }
            Ok(Duration::from_secs(*seconds))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::EvalsFile;

    #[test]
    fn loads_provider_ollama_url_from_evals_toml() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(
            dir.path().join("evals.toml"),
            r#"
[evals]
targets = [{ provider = "ollama", model = "llama3.2:1b" }]

[provider.ollama]
url = "http://localhost:1234"
"#,
        )
        .expect("write evals.toml");

        let file = EvalsFile::load(dir.path()).expect("load evals.toml");

        assert_eq!(
            file.provider.ollama.expect("ollama config").url,
            "http://localhost:1234"
        );
    }

    #[test]
    fn loads_workers_ai_provider_config_from_evals_toml() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(
            dir.path().join("evals.toml"),
            r#"
[evals]
targets = [{ provider = "workers_ai", model = "@cf/meta/llama-3.1-8b-instruct" }]

[provider.workers_ai]
api_token = "token"
account_id = "account"
base_url = "https://example.com/ai/v1"
"#,
        )
        .expect("write evals.toml");

        let file = EvalsFile::load(dir.path()).expect("load evals.toml");
        let workers_ai = file.provider.workers_ai.expect("workers_ai config");

        assert_eq!(workers_ai.api_token.as_deref(), Some("token"));
        assert_eq!(workers_ai.account_id.as_deref(), Some("account"));
        assert_eq!(
            workers_ai.base_url.as_deref(),
            Some("https://example.com/ai/v1")
        );
    }

    #[test]
    fn loads_human_timeout_from_evals_toml() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(
            dir.path().join("evals.toml"),
            r#"
[evals]
timeout = "90s"
targets = [{ provider = "ollama", model = "llama3.2:1b" }]
"#,
        )
        .expect("write evals.toml");

        let file = EvalsFile::load(dir.path()).expect("load evals.toml");

        assert_eq!(file.evals.resolved_timeout, Some(Duration::from_secs(90)));
    }

    #[test]
    fn rejects_timeout_and_timeout_secs_together() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(
            dir.path().join("evals.toml"),
            r#"
[evals]
timeout = "30s"
timeout_secs = 30
targets = [{ provider = "ollama", model = "llama3.2:1b" }]
"#,
        )
        .expect("write evals.toml");

        let error = EvalsFile::load(dir.path()).expect_err("load to fail");
        assert!(
            error
                .to_string()
                .contains("use evals.timeout instead of combining it with evals.timeout_secs")
        );
    }
}
