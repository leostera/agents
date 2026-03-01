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
    port_name: String,
    bot_token: String,
    http: reqwest::Client,
}

#[derive(Clone)]
struct TelegramCommandState {
    exec: ExecEngine,
    port_name: String,
    message: Message,
}

impl PortMessage {
    pub fn from_telegram_text(
        port_name: &str,
        message: &Message,
        text: String,
        input_kind: &str,
    ) -> Option<Self> {
        let chat_id = message.chat.id.0;
        let user_key = TelegramPort::conversation_user_key(message)?;
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
            port: port_name.to_string(),
            user_key,
            text,
            metadata: json!({
                "port": port_name,
                "chat_id": chat_id,
                "chat_type": chat_type,
                "message_id": message.id.0,
                "input_kind": input_kind,
                "thread_id": message.thread_id.map(|thread_id| thread_id.0.0),
                "sender_id": message.from.as_ref().map(|u| u.id.0),
                "sender_username": message.from.as_ref().and_then(|u| u.username.clone()),
                "sender_first_name": message.from.as_ref().map(|u| u.first_name.clone()),
                "sender_last_name": message.from.as_ref().and_then(|u| u.last_name.clone())
            }),
            session_id: None,
            agent_id: None,
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
            PortConfig::Telegram { exec, bot_token } => Self::new(exec, "telegram", bot_token),
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
                    session_id: Some(output.session_id),
                    reply: output.reply,
                    tool_calls: Some(output.tool_calls),
                    error: None,
                    ..message
                },
                Err(err) => PortMessage {
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
    pub fn new(
        exec: ExecEngine,
        port_name: impl Into<String>,
        bot_token: impl Into<String>,
    ) -> Result<Self> {
        let bot_token = bot_token.into();
        Ok(Self {
            exec,
            bot: Bot::new(bot_token.clone()),
            port_name: port_name.into(),
            bot_token,
            http: reqwest::Client::new(),
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
                        port_name: command_port.port_name.clone(),
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
                        if let Some(inbound) = PortMessage::from_telegram_text(
                            &command_port.port_name,
                            &message,
                            text.to_string(),
                            "text",
                        ) && let Some(session_id) = inbound.session_id
                            && let Err(err) = exec
                                .merge_port_message_metadata(
                                    &command_port.port_name,
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

                let inbound = match command_port.inbound_from_telegram_message(&message).await {
                    Ok(Some(inbound)) => inbound,
                    Ok(None) => return Ok(()),
                    Err(err) => {
                        bot.send_message(
                            message.chat.id,
                            format!("Failed to process inbound message: {err}"),
                        )
                        .await?;
                        return Ok(());
                    }
                };

                let port = TelegramPort {
                    exec,
                    bot: bot.clone(),
                    port_name: command_port.port_name.clone(),
                    bot_token: command_port.bot_token.clone(),
                    http: command_port.http.clone(),
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

                    if let Some(session_id) = response.session_id
                        && let Ok(percent) = port
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

                Ok(())
            }
        })
        .await;

        Ok(())
    }

    async fn inbound_from_telegram_message(
        &self,
        message: &Message,
    ) -> Result<Option<PortMessage>> {
        if let Some(text) = message.text() {
            return Ok(PortMessage::from_telegram_text(
                &self.port_name,
                message,
                text.to_string(),
                "text",
            ));
        }

        if let Some(voice) = message.voice() {
            let mime_type = voice
                .mime_type
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "audio/ogg".to_string());
            let audio = self
                .download_telegram_file_bytes(voice.file.id.to_string().as_str())
                .await?;
            let transcript = self.exec.transcribe_audio(audio, mime_type.clone()).await?;
            let transcript = transcript.trim();
            if transcript.is_empty() {
                return Err(anyhow!("voice transcription returned empty text"));
            }

            let text = format!("Voice message transcript:\n{}", transcript);
            let mut inbound =
                PortMessage::from_telegram_text(&self.port_name, message, text, "voice")
                    .ok_or_else(|| anyhow!("failed to build inbound telegram voice message"))?;
            if let Some(metadata) = inbound.metadata.as_object_mut() {
                metadata.insert(
                    "voice_duration_secs".to_string(),
                    json!(voice.duration.seconds()),
                );
                metadata.insert("voice_mime_type".to_string(), json!(mime_type));
            }
            return Ok(Some(inbound));
        }

        Ok(None)
    }

    async fn download_telegram_file_bytes(&self, file_id: &str) -> Result<Vec<u8>> {
        let file = self
            .bot
            .get_file(teloxide::types::FileId(file_id.to_string()))
            .await?;
        let file_url = format!(
            "https://api.telegram.org/file/bot{}/{}",
            self.bot_token, file.path
        );
        let response = self.http.get(file_url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!(
                "telegram file download failed with status {}",
                status
            ));
        }
        Ok(response.bytes().await?.to_vec())
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

    fn conversation_user_key(message: &Message) -> Option<Uri> {
        let chat_id = message.chat.id.0;
        let user_id = message
            .from
            .as_ref()
            .map(|user| user.id.0)
            .unwrap_or(chat_id as u64);
        Uri::from_parts(TELEGRAM_USER_KEY_PREFIX, "user", Some(&user_id.to_string())).ok()
    }
}
