use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::Uri;
use borg_exec::{JsonPortContext, PortContext, SessionOutput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::all::{ChannelId, GatewayIntents, Message};
use serenity::client::{Client, Context, EventHandler};
use serenity::http::Http;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, warn};

use crate::output_format::{format_tool_action_message, split_message_by_limit};
use crate::{Port, PortConfig, PortInput, PortMessage};

const DISCORD_USER_ID_PREFIX: &str = "discord";
const DISCORD_CONVERSATION_PREFIX: &str = "discord";
const DISCORD_MESSAGE_LIMIT: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscordConfig {
    bot_token: String,
    #[serde(default)]
    allowed_external_user_ids: Vec<String>,
}

#[derive(Clone)]
pub struct DiscordPort {
    port_id: Uri,
    port_name: String,
    allows_guests: bool,
    #[allow(dead_code)]
    http: Arc<Http>,
    discord_config: DiscordConfig,
}

#[derive(Clone)]
struct DiscordEventHandler {
    port: DiscordPort,
    inbound: Sender<PortMessage>,
}

#[async_trait]
impl EventHandler for DiscordEventHandler {
    async fn message(&self, _ctx: Context, message: Message) {
        let Some(port_message) = self.port.port_message_from_text(&message) else {
            return;
        };
        if self.inbound.send(port_message).await.is_err() {
            warn!(
                target: "borg_ports",
                port_name = %self.port.port_name,
                "port inbound channel closed"
            );
        }
    }
}

impl DiscordPort {
    fn port_message_from_text(&self, message: &Message) -> Option<PortMessage> {
        if message.author.bot {
            return None;
        }

        let text = message.content.trim().to_string();
        if text.is_empty() {
            return None;
        }

        let user_id = conversation_user_id(message)?;
        let conversation_key = conversation_routing_key(message)?;
        let ctx = discord_session_context_from_message(message);

        if !self.allows_guests
            && !is_allowed_external_user(
                &self.discord_config.allowed_external_user_ids,
                message.author.id.get(),
            )
        {
            return None;
        }

        Some(PortMessage {
            port_id: self.port_id.clone(),
            conversation_key,
            user_id,
            input: PortInput::Chat { text },
            port_context: Arc::new(JsonPortContext::new(ctx)),
        })
    }

    async fn send_output(&self, output: SessionOutput) -> Result<()> {
        let Some(ctx) = output
            .port_context
            .as_any()
            .downcast_ref::<JsonPortContext>()
        else {
            return Ok(());
        };
        let payload = ctx.to_json()?;
        let Some(channel_id) = payload.get("channel_id").and_then(|value| value.as_u64()) else {
            return Ok(());
        };
        let channel = ChannelId::new(channel_id);

        if let Some(reply) = output.reply {
            self.send_text(channel, reply).await?;
        }
        for call in output.tool_calls {
            let body = format_tool_action_message(&call.tool_name, &call.arguments);
            self.send_text(channel, body).await?;
        }
        Ok(())
    }

    async fn send_text(&self, channel_id: ChannelId, message: String) -> Result<()> {
        for chunk in split_message_by_limit(&message, DISCORD_MESSAGE_LIMIT) {
            channel_id.say(&self.http, chunk).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Port for DiscordPort {
    async fn new(port_config: PortConfig) -> Result<Self> {
        let discord_config: DiscordConfig = serde_json::from_value(port_config.settings.clone())?;
        let http = Arc::new(Http::new(&discord_config.bot_token));
        Ok(Self {
            port_id: port_config.port_id.clone(),
            port_name: port_config.port_name,
            allows_guests: matches!(port_config.privacy, crate::port::Privacy::Public),
            http,
            discord_config,
        })
    }

    async fn run(
        self,
        inbound: Sender<PortMessage>,
        mut outbound: Receiver<SessionOutput>,
    ) -> Result<()> {
        let outbound_port = self.clone();
        let outbound_task = tokio::spawn(async move {
            while let Some(output) = outbound.recv().await {
                if let Err(err) = outbound_port.send_output(output).await {
                    error!(
                        target: "borg_ports",
                        port_name = %outbound_port.port_name,
                        error = %err,
                        "failed to send discord output"
                    );
                }
            }
        });

        let intents = GatewayIntents::GUILDS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        let handler = DiscordEventHandler {
            port: self.clone(),
            inbound,
        };
        let mut client = Client::builder(&self.discord_config.bot_token, intents)
            .event_handler(handler)
            .await
            .map_err(|err| anyhow!("failed to create discord client: {err}"))?;

        let start_result = client
            .start()
            .await
            .map_err(|err| anyhow!("discord client failed: {err}"));
        outbound_task.abort();
        start_result
    }
}

fn conversation_user_id(message: &Message) -> Option<Uri> {
    Uri::from_parts(
        DISCORD_USER_ID_PREFIX,
        "user",
        Some(&message.author.id.get().to_string()),
    )
    .ok()
}

fn conversation_routing_key(message: &Message) -> Option<Uri> {
    Uri::from_parts(
        DISCORD_CONVERSATION_PREFIX,
        "conversation",
        Some(&message.channel_id.get().to_string()),
    )
    .ok()
}

fn discord_session_context_from_message(message: &Message) -> serde_json::Value {
    json!({
        "provider": "discord",
        "channel_id": message.channel_id.get(),
        "guild_id": message.guild_id.map(|id| id.get()),
        "message_id": message.id.get(),
        "author_id": message.author.id.get(),
        "author_name": message.author.name.clone(),
    })
}

fn is_allowed_external_user(allowed_external_user_ids: &[String], author_id: u64) -> bool {
    if allowed_external_user_ids.is_empty() {
        return false;
    }
    let author_id = author_id.to_string();
    allowed_external_user_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .any(|value| value == author_id)
}

#[cfg(test)]
mod tests {
    use super::is_allowed_external_user;

    #[test]
    fn allowlist_matches_numeric_discord_id() {
        let allowed = vec!["123456789012345678".to_string()];
        assert!(is_allowed_external_user(&allowed, 123456789012345678));
    }

    #[test]
    fn allowlist_rejects_missing_discord_id() {
        let allowed = vec!["123456789012345678".to_string()];
        assert!(!is_allowed_external_user(&allowed, 111));
    }
}
