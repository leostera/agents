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
        let settings = ProviderConfigSnapshot {
            openai_api_key: self.db.get_provider_api_key(OPENAI_PROVIDER).await?,
            openai_base_url: None,
            openai_api_mode: None,
            openrouter_api_key: self.db.get_provider_api_key(OPENROUTER_PROVIDER).await?,
            openrouter_base_url: None,
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
            "provider supervisor reconciled settings"
        );

        Ok(())
    }

    pub async fn shutdown(&self) {}
}
