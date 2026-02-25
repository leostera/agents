use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::Uri;
use borg_exec::{ExecEngine, UserMessage};
use serde_json::json;
use teloxide::prelude::*;

use crate::{Port, PortConfig, PortMessage};

const TELEGRAM_USER_KEY_PREFIX: &str = "telegram";

#[derive(Clone)]
pub struct TelegramPort {
    exec: ExecEngine,
    bot: Bot,
}

impl PortMessage {
    pub fn from_telegram(message: &Message) -> Option<Self> {
        let text = message.text()?.to_string();
        let chat_id = message.chat.id.0;
        let user_id = message.from.as_ref().map(|user| user.id.0).unwrap_or(chat_id as u64);
        let session_id = Uri::from_parts("borg", "session", Some(&format!("telegram_{chat_id}"))).ok();

        Some(Self {
            user_key: Uri::from_parts(
                TELEGRAM_USER_KEY_PREFIX,
                "user",
                Some(&user_id.to_string()),
            )
            .ok()?,
            text,
            metadata: json!({
                "port": "telegram",
                "chat_id": chat_id,
                "message_id": message.id.0
            }),
            session_id,
            agent_id: None,
            task_id: None,
            error: None,
        })
    }
}

#[async_trait]
impl Port for TelegramPort {
    fn init(config: PortConfig) -> Result<Self> {
        match config {
            PortConfig::Telegram { exec, bot_token } => Ok(Self {
                exec,
                bot: Bot::new(bot_token),
            }),
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

            let outbound = match self
                .exec
                .enqueue_user_message(inbox, message.session_id.clone())
                .await
            {
                Ok((task_id, session_id)) => PortMessage {
                    task_id: Some(task_id.to_string()),
                    session_id: Some(session_id),
                    error: None,
                    ..message
                },
                Err(err) => PortMessage {
                    task_id: None,
                    session_id: message.session_id,
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
    pub async fn run(self) -> Result<()> {
        let exec = self.exec.clone();
        let bot = self.bot.clone();

        teloxide::repl(bot, move |bot: Bot, message: Message| {
            let exec = exec.clone();
            async move {
                let Some(inbound) = PortMessage::from_telegram(&message) else {
                    return Ok(());
                };

                let port = TelegramPort {
                    exec,
                    bot: bot.clone(),
                };

                let mut outbound = port.handle_messages(vec![inbound]).await;
                if let Some(response) = outbound.pop() {
                    let reply = match response.error {
                        Some(error) => format!("Failed to queue message: {error}"),
                        None => match response.task_id {
                            Some(task_id) => format!("Queued task {task_id}"),
                            None => "Queued message".to_string(),
                        },
                    };
                    bot.send_message(message.chat.id, reply).await?;
                }

                Ok(())
            }
        })
        .await;

        Ok(())
    }
}

pub fn init_telegram_port(exec: ExecEngine, bot_token: impl Into<String>) -> Result<TelegramPort> {
    TelegramPort::init(PortConfig::Telegram {
        exec,
        bot_token: bot_token.into(),
    })
}
