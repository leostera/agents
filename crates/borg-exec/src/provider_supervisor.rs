use std::{sync::Arc, time::Duration};

use anyhow::Result;
use borg_db::BorgDb;
use tokio::{sync::RwLock, task::JoinHandle, time};
use tracing::{error, info};

use crate::ProviderConfigSnapshot;

const OPENAI_PROVIDER: &str = "openai";
const OPENROUTER_PROVIDER: &str = "openrouter";
const RUNTIME_SETTINGS_PORT: &str = "runtime";
const RUNTIME_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct ProviderConfigSupervisor {
    db: BorgDb,
    shared_settings: Arc<RwLock<ProviderConfigSnapshot>>,
    poll_interval: Duration,
    enabled: bool,
}

impl ProviderConfigSupervisor {
    pub fn new(db: BorgDb, shared_settings: Arc<RwLock<ProviderConfigSnapshot>>) -> Self {
        Self {
            db,
            shared_settings,
            poll_interval: DEFAULT_POLL_INTERVAL,
            enabled: true,
        }
    }

    pub fn disabled(db: BorgDb, shared_settings: Arc<RwLock<ProviderConfigSnapshot>>) -> Self {
        Self {
            db,
            shared_settings,
            poll_interval: DEFAULT_POLL_INTERVAL,
            enabled: false,
        }
    }

    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            if !self.enabled {
                return;
            }

            if let Err(err) = self.reconcile_now().await {
                error!(target: "borg_exec", error = %err, "provider supervisor initial reconcile failed");
            }

            let mut ticker = time::interval(self.poll_interval);
            loop {
                ticker.tick().await;
                if let Err(err) = self.reconcile_now().await {
                    error!(target: "borg_exec", error = %err, "provider supervisor reconcile failed");
                }
            }
        })
    }

    pub async fn on_provider_setting_changed(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.reconcile_now().await
    }

    pub async fn reconcile_now(&self) -> Result<()> {
        let openai = self.db.get_provider(OPENAI_PROVIDER).await?;
        let openrouter = self.db.get_provider(OPENROUTER_PROVIDER).await?;

        let settings = ProviderConfigSnapshot {
            openai_api_key: openai
                .as_ref()
                .filter(|provider| provider.enabled)
                .map(|provider| provider.api_key.trim().to_string())
                .filter(|api_key| !api_key.is_empty()),
            openai_base_url: None,
            openai_api_mode: None,
            openai_default_text_model: openai
                .as_ref()
                .and_then(|provider| provider.default_text_model.clone())
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
            openai_default_audio_model: openai
                .as_ref()
                .and_then(|provider| provider.default_audio_model.clone())
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
            openrouter_api_key: openrouter
                .as_ref()
                .filter(|provider| provider.enabled)
                .map(|provider| provider.api_key.trim().to_string())
                .filter(|api_key| !api_key.is_empty()),
            openrouter_base_url: None,
            openrouter_default_text_model: openrouter
                .as_ref()
                .and_then(|provider| provider.default_text_model.clone())
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
            openrouter_default_audio_model: openrouter
                .as_ref()
                .and_then(|provider| provider.default_audio_model.clone())
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
            preferred_provider: self
                .db
                .get_port_setting(RUNTIME_SETTINGS_PORT, RUNTIME_PREFERRED_PROVIDER_KEY)
                .await?
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };

        {
            let mut guard = self.shared_settings.write().await;
            *guard = settings.clone();
        }

        info!(
            target: "borg_exec",
            preferred_provider = ?settings.preferred_provider,
            has_openai = settings.openai_api_key.is_some(),
            has_openrouter = settings.openrouter_api_key.is_some(),
            openai_default_text_model = ?settings.openai_default_text_model,
            openai_default_audio_model = ?settings.openai_default_audio_model,
            openrouter_default_text_model = ?settings.openrouter_default_text_model,
            openrouter_default_audio_model = ?settings.openrouter_default_audio_model,
            "provider supervisor reconciled settings"
        );

        Ok(())
    }

    pub async fn shutdown(&self) {}
}
