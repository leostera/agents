use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::Uri;
use borg_exec::{ExecEngine, UserMessage};
use serde_json::json;
use teloxide::prelude::*;

use crate::{Port, PortConfig, PortMessage};

mod commands;
mod context_sync;
mod formatting;
mod typing;

use commands::build_telegram_command_registry;
use typing::TypingLoop;

const TELEGRAM_USER_KEY_PREFIX: &str = "telegram";
const TELEGRAM_MESSAGE_LIMIT: usize = 4000;
const TELEGRAM_START_GREETING: &str =
    "Hi! I am Borg. Send me a message and I will reply in this chat.";
const TELEGRAM_CONTEXT_MAX_TOKENS: usize = 128_000;
const TELEGRAM_TYPING_REFRESH_SECS: u64 = 4;

#[derive(Clone)]
pub struct TelegramPort {
    exec: ExecEngine,
    bot: Bot,
}

#[derive(Clone)]
struct TelegramCommandState {
    exec: ExecEngine,
    message: Message,
}

impl PortMessage {
    pub fn from_telegram(message: &Message) -> Option<Self> {
        let text = message.text()?.to_string();
        let chat_id = message.chat.id.0;
        let user_id = message
            .from
            .as_ref()
            .map(|user| user.id.0)
            .unwrap_or(chat_id as u64);
        let session_id =
            Uri::from_parts("borg", "session", Some(&format!("telegram_{chat_id}"))).ok();
        let chat_type = if message.chat.is_private() {
            "private"
        } else if message.chat.is_group() {
            "group"
        } else if message.chat.is_supergroup() {
            "supergroup"
        } else if message.chat.is_channel() {
            "channel"
        } else {
            "unknown"
        };

        Some(Self {
            port: "telegram".to_string(),
            user_key: Uri::from_parts(TELEGRAM_USER_KEY_PREFIX, "user", Some(&user_id.to_string()))
                .ok()?,
            text,
            metadata: json!({
                "port": "telegram",
                "chat_id": chat_id,
                "chat_type": chat_type,
                "message_id": message.id.0,
                "thread_id": message.thread_id.map(|thread_id| thread_id.0.0),
                "sender_id": message.from.as_ref().map(|u| u.id.0),
                "sender_username": message.from.as_ref().and_then(|u| u.username.clone()),
                "sender_first_name": message.from.as_ref().map(|u| u.first_name.clone()),
                "sender_last_name": message.from.as_ref().and_then(|u| u.last_name.clone())
            }),
            session_id,
            agent_id: None,
            task_id: None,
            reply: None,
            tool_calls: None,
            error: None,
        })
    }
}

#[async_trait]
impl Port for TelegramPort {
    fn init(config: PortConfig) -> Result<Self> {
        match config {
            PortConfig::Telegram { exec, bot_token } => Self::new(exec, bot_token),
            _ => Err(anyhow!("invalid config for TelegramPort")),
        }
    }

    async fn handle_messages(&self, messages: Vec<PortMessage>) -> Vec<PortMessage> {
        let mut out = Vec::with_capacity(messages.len());
        for message in messages {
            let inbox = UserMessage {
                user_key: message.user_key.clone(),
                text: message.text.clone(),
                session_id: message.session_id.clone(),
                agent_id: message.agent_id.clone(),
                metadata: message.metadata.clone(),
            };

            let outbound = match self.exec.process_port_message(&message.port, inbox).await {
                Ok(output) => PortMessage {
                    task_id: None,
                    session_id: Some(output.session_id),
                    reply: output.reply,
                    tool_calls: Some(output.tool_calls),
                    error: None,
                    ..message
                },
                Err(err) => PortMessage {
                    task_id: None,
                    session_id: message.session_id,
                    reply: None,
                    tool_calls: None,
                    error: Some(err.to_string()),
                    ..message
                },
            };
            out.push(outbound);
        }
        out
    }
}

impl TelegramPort {
    pub fn new(exec: ExecEngine, bot_token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            exec,
            bot: Bot::new(bot_token),
        })
    }

    pub async fn run(self) -> Result<()> {
        let port = self.clone();
        if let Err(err) = port.refresh_session_contexts().await {
            tracing::warn!(
                target: "borg_ports",
                error = %err,
                "failed refreshing telegram session contexts at startup"
            );
        }

        teloxide::repl(self.bot.clone(), move |bot: Bot, message: Message| {
            let command_port = port.clone();
            let exec = command_port.exec.clone();
            async move {
                let _typing = TypingLoop::start(bot.clone(), message.chat.id);

                if let Some(text) = message.text() {
                    let command_state = TelegramCommandState {
                        exec: exec.clone(),
                        message: message.clone(),
                    };
                    let commands = match build_telegram_command_registry(command_state) {
                        Ok(value) => value,
                        Err(err) => {
                            bot.send_message(
                                message.chat.id,
                                format!("Failed to load command registry: {err}"),
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    if commands.is_command(text) {
                        if let Some(inbound) = PortMessage::from_telegram(&message) {
                            if let Some(session_id) = inbound.session_id {
                                if let Err(err) = exec
                                    .merge_port_message_metadata(
                                        "telegram",
                                        &session_id,
                                        &inbound.metadata,
                                    )
                                    .await
                                {
                                    bot.send_message(
                                        message.chat.id,
                                        format!("Failed to update session context: {err}"),
                                    )
                                    .await?;
                                    return Ok(());
                                }
                            }
                        }
                        let response = if Self::is_help_command(text) {
                            commands.help()
                        } else {
                            match commands.run(text).await {
                                Ok(Some(value)) => value,
                                Ok(None) => String::new(),
                                Err(err) => format!("Command error: {err}"),
                            }
                        };
                        if !response.is_empty() {
                            command_port.send_text(message.chat.id, response).await?;
                        }
                        return Ok(());
                    }
                }

                let Some(inbound) = PortMessage::from_telegram(&message) else {
                    return Ok(());
                };

                let port = TelegramPort {
                    exec,
                    bot: bot.clone(),
                };

                let mut outbound = port.handle_messages(vec![inbound]).await;
                if let Some(response) = outbound.pop() {
                    if let Some(error) = response.error {
                        bot.send_message(
                            message.chat.id,
                            format!("Failed to process message: {error}"),
                        )
                        .await?;
                        return Ok(());
                    }

                    if let Some(tool_calls) = response.tool_calls {
                        for call in tool_calls {
                            let formatted = port.format_tool_action_message(&call);
                            port.send_text(message.chat.id, formatted).await?;
                            if call.is_error() {
                                let output = format!("Result:\n{}", call.output_message().trim());
                                port.send_text(message.chat.id, output).await?;
                            }
                        }
                    }

                    let reply = response
                        .reply
                        .unwrap_or_else(|| "Message processed, no reply generated.".to_string());
                    port.send_text(message.chat.id, reply).await?;

                    if let Some(session_id) = response.session_id {
                        if let Ok(percent) = port
                            .exec
                            .estimate_session_context_usage_percent(
                                &session_id,
                                TELEGRAM_CONTEXT_MAX_TOKENS,
                            )
                            .await
                        {
                            let usage_line = format!("Context: ~{}% used", percent);
                            bot.send_message(message.chat.id, usage_line).await?;
                        }
                    }
                }

                Ok(())
            }
        })
        .await;

        Ok(())
    }

    fn is_help_command(input: &str) -> bool {
        let Some(first_token) = input.split_whitespace().next() else {
            return false;
        };
        if !first_token.starts_with('/') {
            return false;
        }
        let command = first_token.trim_start_matches('/').split('@').next();
        matches!(command, Some("help"))
    }
}
