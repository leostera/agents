use std::path::Path;

use anyhow::{Context, Result, bail};
use borg_llm::completion::ProviderType;
use config::{Config, File, FileFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EvalsFile {
    pub evals: EvalsConfig,
}

#[derive(Debug, Deserialize)]
pub struct EvalsConfig {
    #[serde(default = "default_trials")]
    pub trials: usize,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Deserialize)]
pub struct TargetConfig {
    pub label: Option<String>,
    pub provider: String,
    pub model: String,
    pub concurrency: Option<usize>,
}

impl EvalsFile {
    pub fn load(workspace_root: &Path) -> Result<Self> {
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

    fn validate(&mut self) -> Result<()> {
        for target in &mut self.evals.targets {
            target.validate()?;
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
