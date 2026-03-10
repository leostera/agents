use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

use borg_db::BorgDb;
use borg_llm::BorgLLM;
use borg_llm::providers::openai::{OpenAiApiMode, OpenAiProvider};
use borg_llm::providers::openrouter::OpenRouterProvider;

const RUNTIME_PORT: &str = "runtime";
const PREFERRED_PROVIDER_ID_KEY: &str = "preferred_provider_id";
const LEGACY_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";

pub struct BorgLLMResolver {
    db: BorgDb,
    cached_llm: Arc<RwLock<Option<Arc<BorgLLM>>>>,
}

impl BorgLLMResolver {
    pub fn new(db: BorgDb) -> Self {
        Self {
            db,
            cached_llm: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn llm(&self) -> Result<Arc<BorgLLM>> {
        if let Some(llm) = self.cached_llm.read().unwrap().as_ref() {
            return Ok(llm.clone());
        }

        // Initial load if not cached yet
        let llm = Arc::new(self.resolve_llm_from_db().await?);
        *self.cached_llm.write().unwrap() = Some(llm.clone());
        Ok(llm)
    }

    pub async fn start_update_loop(self: Arc<Self>) {
        info!(target: "borg_exec", "starting llm resolver update loop");
        let mut ticker = interval(Duration::from_millis(500));
        loop {
            ticker.tick().await;
            if let Err(err) = self.update_cache().await {
                error!(target: "borg_exec", error = %err, "failed to update llm resolver cache");
            }
        }
    }

    async fn update_cache(&self) -> Result<()> {
        // Simple polling for now
        let llm = Arc::new(self.resolve_llm_from_db().await?);
        *self.cached_llm.write().unwrap() = Some(llm);
        Ok(())
    }

    async fn resolve_llm_from_db(&self) -> Result<BorgLLM> {
        let settings = self.load_provider_settings().await?;

        let mut builder = BorgLLM::builder();
        for provider in ordered_providers(&settings) {
            match provider.provider_kind.as_str() {
                "openai" => {
                    if let Some(openai) = self.build_openai_provider(provider)? {
                        builder = builder.add_provider(openai);
                    }
                }
                "openrouter" => {
                    if let Some(openrouter) = self.build_openrouter_provider(provider)? {
                        builder = builder.add_provider(openrouter);
                    }
                }
                "lmstudio" => {
                    if let Some(lmstudio) = self.build_lmstudio_provider(provider)? {
                        builder = builder.add_provider(lmstudio);
                    }
                }
                "ollama" => {
                    if let Some(ollama) = self.build_ollama_provider(provider)? {
                        builder = builder.add_provider(ollama);
                    }
                }
                _ => {}
            }
        }

        Ok(builder.build()?)
    }

    async fn load_provider_settings(&self) -> Result<ProviderSettings> {
        let preferred_provider_id = self
            .db
            .get_port_setting(RUNTIME_PORT, PREFERRED_PROVIDER_ID_KEY)
            .await?
            .or(self
                .db
                .get_port_setting(RUNTIME_PORT, LEGACY_PREFERRED_PROVIDER_KEY)
                .await?);

        let providers = self
            .db
            .list_providers(64)
            .await?
            .into_iter()
            .filter(|provider| provider.enabled)
            .filter_map(|provider| {
                Some(ProviderSetting {
                    provider_id: provider.provider,
                    provider_kind: provider.provider_kind.trim().to_ascii_lowercase(),
                    api_key: normalize_optional_text(Some(provider.api_key)),
                    base_url: normalize_optional_text(provider.base_url),
                    default_text_model: provider.default_text_model,
                    default_audio_model: provider.default_audio_model,
                })
            })
            .filter(|provider| !provider.provider_kind.is_empty())
            .collect::<Vec<_>>();

        Ok(ProviderSettings {
            providers,
            preferred_provider_id: normalize_optional_text(preferred_provider_id),
        })
    }

    fn build_openai_provider(&self, provider: &ProviderSetting) -> Result<Option<OpenAiProvider>> {
        let Some(api_key) = &provider.api_key else {
            return Ok(None);
        };

        let mut builder = OpenAiProvider::build()
            .api_key(api_key.clone())
            .provider_name("openai")
            .api_mode(OpenAiApiMode::ChatCompletions);
        if let Some(base_url) = &provider.base_url {
            builder = builder.base_url(base_url.clone());
        }
        if let Some(model) = &provider.default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }
        if let Some(model) = &provider.default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }

    fn build_openrouter_provider(
        &self,
        provider: &ProviderSetting,
    ) -> Result<Option<OpenRouterProvider>> {
        let Some(api_key) = &provider.api_key else {
            return Ok(None);
        };

        let mut builder = OpenRouterProvider::build().api_key(api_key.clone());
        if let Some(base_url) = &provider.base_url {
            builder = builder.base_url(base_url.clone());
        }
        if let Some(model) = &provider.default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }
        if let Some(model) = &provider.default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }

    fn build_lmstudio_provider(
        &self,
        provider: &ProviderSetting,
    ) -> Result<Option<OpenAiProvider>> {
        let Some(base_url) = &provider.base_url else {
            return Ok(None);
        };

        let api_key = provider
            .api_key
            .clone()
            .unwrap_or_else(|| "local-lmstudio".to_string());
        let mut builder = OpenAiProvider::build()
            .api_key(api_key)
            .provider_name("lmstudio")
            .base_url(base_url.clone())
            .api_mode(OpenAiApiMode::ChatCompletions);
        if let Some(model) = &provider.default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }
        if let Some(model) = &provider.default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }

    fn build_ollama_provider(&self, provider: &ProviderSetting) -> Result<Option<OpenAiProvider>> {
        let Some(base_url) = &provider.base_url else {
            return Ok(None);
        };

        let api_key = provider
            .api_key
            .clone()
            .unwrap_or_else(|| "local-ollama".to_string());
        let mut builder = OpenAiProvider::build()
            .api_key(api_key)
            .provider_name("ollama")
            .base_url(base_url.clone())
            .api_mode(OpenAiApiMode::ChatCompletions);
        if let Some(model) = &provider.default_text_model {
            builder = builder.chat_completions_model(model.clone());
        }
        if let Some(model) = &provider.default_audio_model {
            builder = builder.audio_transcriptions_model(model.clone());
        }

        Ok(Some(builder.build()?))
    }
}

#[derive(Clone, Default)]
struct ProviderSettings {
    providers: Vec<ProviderSetting>,
    preferred_provider_id: Option<String>,
}

#[derive(Clone, Debug)]
struct ProviderSetting {
    provider_id: String,
    provider_kind: String,
    api_key: Option<String>,
    base_url: Option<String>,
    default_text_model: Option<String>,
    default_audio_model: Option<String>,
}

fn ordered_providers(settings: &ProviderSettings) -> Vec<&ProviderSetting> {
    let mut ordered = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(preferred_provider_id) = settings.preferred_provider_id.as_deref() {
        for provider in &settings.providers {
            let preferred_matches_id = provider.provider_id == preferred_provider_id;
            let preferred_matches_kind = provider.provider_kind == preferred_provider_id;
            if preferred_matches_id || preferred_matches_kind {
                ordered.push(provider);
                seen.insert(provider.provider_id.as_str());
                break;
            }
        }
    }

    for provider in &settings.providers {
        if !seen.contains(provider.provider_id.as_str()) {
            ordered.push(provider);
            seen.insert(provider.provider_id.as_str());
        }
    }

    ordered
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}
