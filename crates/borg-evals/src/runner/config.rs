use std::path::Path;

use crate::{
    ExecutionTarget, OllamaProviderConfig as PublicOllamaProviderConfig, ProviderConfigs, RunConfig,
};
use anyhow::{Context, Result, bail};
use borg_llm::completion::ProviderType;
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
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
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
}

#[derive(Debug, Deserialize)]
pub(super) struct OllamaProviderConfig {
    pub url: String,
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
        }
    }

    fn validate(&mut self) -> Result<()> {
        for target in &mut self.evals.targets {
            target.validate()?;
        }
        if let Some(ollama) = &mut self.provider.ollama {
            ollama.url = ollama.url.trim().to_string();
            if ollama.url.is_empty() {
                bail!("provider.ollama.url cannot be empty");
            }
        }
        Ok(())
    }
}

impl TargetConfig {
    fn validate(&mut self) -> Result<()> {
        let provider = self.provider.trim();
        if !supported_provider(provider) {
            bail!(
                "unsupported eval target provider {:?}; expected one of: openai, anthropic, openrouter, lm_studio, ollama, apple",
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

#[cfg(test)]
mod tests {
    use std::fs;

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
}
