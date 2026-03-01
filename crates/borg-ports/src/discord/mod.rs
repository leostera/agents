use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serenity::http::Http;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{Port, PortConfig, PortMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscordConfig {
    bot_token: String,
    #[serde(default)]
    allowed_external_user_ids: Vec<String>,
}

#[derive(Clone)]
pub struct DiscordPort {
    port_name: String,
    #[allow(dead_code)]
    http: Arc<Http>,
    #[allow(dead_code)]
    discord_config: DiscordConfig,
}

#[async_trait]
impl Port for DiscordPort {
    async fn new(port_config: PortConfig) -> Result<Self> {
        let discord_config: DiscordConfig = serde_json::from_value(port_config.settings.clone())?;
        let http = Arc::new(Http::new(&discord_config.bot_token));
        Ok(Self {
            port_name: port_config.port_name,
            http,
            discord_config,
        })
    }

    async fn run(
        self,
        _inbound: Sender<PortMessage>,
        _outbound: Receiver<borg_exec::SessionOutput>,
    ) -> Result<()> {
        Err(anyhow!(
            "discord port `{}` is not implemented yet",
            self.port_name
        ))
    }
}
