use anyhow::{Result, anyhow};
use borg_db::BorgDb;
use borg_llm::BorgLLM;
use borg_llm::providers::openai::{OpenAiApiMode, OpenAiProvider};
use borg_llm::providers::openrouter::OpenRouterProvider;

pub struct BorgLLMResolver {
    db: BorgDb,
}

impl BorgLLMResolver {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    pub async fn llm(&self) -> Result<BorgLLM> {
        let settings = self.load_provider_settings().await?;

        let mut builder = BorgLLM::build();
        for name in ordered_provider_names(&settings) {
            match name.as_str() {
                "openai" => {
                    if let Some(openai) = self.build_openai_provider(&settings)? {
                        builder = builder.add_provider(openai);
                    }
                }
                "openrouter" => {
                    if let Some(openrouter) = self.build_openrouter_provider(&settings)? {
                        builder = builder.add_provider(openrouter);
                    }
                }
                _ => {}
            }
        }

        Ok(builder.build()?)
    }

    async fn load_provider_settings(&self) -> Result<ProviderSettings> {
        let mut settings = ProviderSettings::default();

        let providers = self.db.list_providers(16).await?;
        for provider in providers {
            if !provider.enabled {
                continue;
            }
            match provider.provider.as_str() {
                "openai" => {
                    settings.openai_api_key = Some(provider.api_key);
                    settings.openai_default_text_model = provider.default_text_model;
                    settings.openai_default_audio_model = provider.default_audio_model;
                }
                "openrouter" => {
                    settings.openrouter_api_key = Some(provider.api_key);
                    settings.openrouter_default_text_model = provider.default_text_model;
                    settings.openrouter_default_audio_model = provider.default_audio_model;
                }
                _ => {}
            }
        }

        settings.preferred_provider = self
            .db
            .get_port_setting("runtime", "preferred_provider")
            .await?;

        Ok(settings)
    }

    fn build_openai_provider(&self, settings: &ProviderSettings) -> Result<Option<OpenAiProvider>> {
        let Some(api_key) = &settings.openai_api_key else {
            return Ok(None);
        };

        let api_mode = settings
            .openai_api_mode
            .clone()
            .unwrap_or_else(|| "chat".to_string());

        let api_mode = match api_mode.to_lowercase().as_str() {
            "chat" | "chat_completions" => OpenAiApiMode::ChatCompletions,
            "completions" => OpenAiApiMode::Completions,
            _ => return Err(anyhow!("unsupported openai api mode: {}", api_mode)),
        };

        let mut builder = OpenAiProvider::build()
            .api_key(api_key.clone())
            .api_mode(api_mode);

        if let Some(model) = &settings.openai_default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }

        if let Some(model) = &settings.openai_default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }

    fn build_openrouter_provider(
        &self,
        settings: &ProviderSettings,
    ) -> Result<Option<OpenRouterProvider>> {
        let Some(api_key) = &settings.openrouter_api_key else {
            return Ok(None);
        };

        let mut builder = OpenRouterProvider::build().api_key(api_key.clone());

        if let Some(model) = &settings.openrouter_default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }

        if let Some(model) = &settings.openrouter_default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }
}

#[derive(Clone, Default)]
struct ProviderSettings {
    openai_api_key: Option<String>,
    openai_api_mode: Option<String>,
    openai_default_text_model: Option<String>,
    openai_default_audio_model: Option<String>,
    openrouter_api_key: Option<String>,
    openrouter_default_text_model: Option<String>,
    openrouter_default_audio_model: Option<String>,
    preferred_provider: Option<String>,
}

fn ordered_provider_names(settings: &ProviderSettings) -> Vec<String> {
    let mut available = Vec::new();
    if settings.openai_api_key.is_some() {
        available.push("openai".to_string());
    }
    if settings.openrouter_api_key.is_some() {
        available.push("openrouter".to_string());
    }

    let preferred = settings.preferred_provider.as_deref().unwrap_or("openai");

    let mut ordered = Vec::new();
    if available.iter().any(|value| value == preferred) {
        ordered.push(preferred.to_string());
    }
    for name in available {
        if !ordered.iter().any(|value| value == &name) {
            ordered.push(name);
        }
    }

    ordered
}
