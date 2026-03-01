use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use borg_core::{TelegramUserId, Uri};
use borg_exec::{SessionOutput, TelegramSessionContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use teloxide::prelude::*;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, warn};

use crate::{Port, PortConfig, PortMessage};

const TELEGRAM_USER_ID_PREFIX: &str = "telegram";
const TELEGRAM_CONVERSATION_PREFIX: &str = "telegram";
const TELEGRAM_MESSAGE_LIMIT: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramConfig {
    bot_token: String,
    #[serde(default)]
    allowed_external_user_ids: Vec<String>,
}

#[derive(Clone)]
pub struct TelegramPort {
    port_id: Uri,
    port_name: String,
    allows_guests: bool,
    bot: Bot,
    telegram_config: TelegramConfig,
}

impl TelegramPort {
    fn port_message_from_text(&self, message: &Message) -> Option<PortMessage> {
        let text = message.text()?.to_string();
        let user_id = conversation_user_id(message)?;
        let conversation_key = conversation_routing_key(message)?;

        let ctx = telegram_session_context_from_message(message);

        if !self.allows_guests
            && !is_allowed_external_user(&self.telegram_config.allowed_external_user_ids, &ctx)
        {
            return None;
        }

        Some(PortMessage {
            port_id: self.port_id.clone(),
            conversation_key,
            user_id,
            text,
            port_context: Arc::new(ctx),
        })
    }

    async fn send_output(&self, output: SessionOutput) -> Result<()> {
        let Some(ctx) = output
            .port_context
            .as_any()
            .downcast_ref::<TelegramSessionContext>()
        else {
            return Ok(());
        };

        let chat_id = ChatId(ctx.chat_id);

        if let Some(reply) = output.reply {
            self.send_text(chat_id, reply).await?;
        }

        for call in output.tool_calls {
            let body = format_tool_action_message(&call.tool_name, &call.arguments);
            self.send_text(chat_id, body).await?;
        }

        Ok(())
    }

    async fn send_text(&self, chat_id: ChatId, message: String) -> Result<()> {
        for chunk in split_message(&message) {
            self.bot.send_message(chat_id, chunk).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Port for TelegramPort {
    async fn new(port_config: PortConfig) -> Result<Self> {
        let telegram_config: TelegramConfig = serde_json::from_value(port_config.settings.clone())?;
        Ok(Self {
            port_id: port_config.port_id.clone(),
            port_name: port_config.port_name,
            allows_guests: matches!(port_config.privacy, crate::port::Privacy::Public),
            bot: Bot::new(telegram_config.bot_token.clone()),
            telegram_config,
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
                        "failed to send telegram output"
                    );
                }
            }
        });

        let inbound_port = self.clone();
        let inbound_tx = inbound.clone();
        let bot = self.bot.clone();

        teloxide::repl(bot, move |_bot: Bot, message: Message| {
            let inbound_port = inbound_port.clone();
            let inbound_tx = inbound_tx.clone();
            async move {
                if let Some(port_message) = inbound_port.port_message_from_text(&message)
                    && inbound_tx.send(port_message).await.is_err()
                {
                    warn!(
                        target: "borg_ports",
                        port_name = %inbound_port.port_name,
                        "port inbound channel closed"
                    );
                }
                respond(())
            }
        })
        .await;

        outbound_task.abort();
        Ok(())
    }
}

fn split_message(message: &str) -> Vec<String> {
    if message.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = String::new();
    for line in message.lines() {
        if current.len() + line.len() + 1 > TELEGRAM_MESSAGE_LIMIT {
            out.push(current.trim_end().to_string());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        out.push(current.trim_end().to_string());
    }
    out
}

fn conversation_user_id(message: &Message) -> Option<Uri> {
    let chat_id = message.chat.id.0;
    let user_id = message
        .from
        .as_ref()
        .map(|user| user.id.0)
        .unwrap_or(chat_id as u64);
    Uri::from_parts(TELEGRAM_USER_ID_PREFIX, "user", Some(&user_id.to_string())).ok()
}

fn conversation_routing_key(message: &Message) -> Option<Uri> {
    let chat_id = message.chat.id.0;
    routing_key_for_chat_id(chat_id)
}

fn telegram_session_context_from_message(message: &Message) -> TelegramSessionContext {
    let mut ctx = TelegramSessionContext::default();
    ctx.set_chat(message.chat.id.0, chat_type_label(message));
    ctx.set_last_message_refs(
        Some(i64::from(message.id.0)),
        message.thread_id.map(|thread_id| i64::from(thread_id.0.0)),
    );

    if let Some(sender) = &message.from {
        ctx.upsert_participant(
            sender.id.0,
            sender.username.clone(),
            Some(sender.first_name.clone()),
            sender.last_name.clone(),
        );
    }

    ctx
}

fn chat_type_label(message: &Message) -> &'static str {
    if message.chat.is_private() {
        "private"
    } else if message.chat.is_group() {
        "group"
    } else if message.chat.is_supergroup() {
        "supergroup"
    } else if message.chat.is_channel() {
        "channel"
    } else {
        "unknown"
    }
}

fn routing_key_for_chat_id(chat_id: i64) -> Option<Uri> {
    Uri::from_parts(
        TELEGRAM_CONVERSATION_PREFIX,
        "conversation",
        Some(&chat_id.to_string()),
    )
    .ok()
}

fn is_allowed_external_user(
    allowed_external_user_ids: &[String],
    ctx: &TelegramSessionContext,
) -> bool {
    if allowed_external_user_ids.is_empty() {
        return false;
    }

    let candidates = telegram_candidates(ctx);
    if candidates.is_empty() {
        return false;
    }

    allowed_external_user_ids
        .iter()
        .filter_map(|raw| raw.parse::<TelegramUserId>().ok())
        .any(|allowed| candidates.iter().any(|candidate| candidate == &allowed))
}

fn telegram_candidates(ctx: &TelegramSessionContext) -> Vec<TelegramUserId> {
    let mut out = Vec::new();
    for participant in ctx.participants.values() {
        if let Ok(id) = participant.id.parse::<u64>() {
            out.push(TelegramUserId::from_sender_id(id));
        }
        if let Some(username) = &participant.username
            && let Some(parsed) = TelegramUserId::from_sender_username(username)
        {
            out.push(parsed);
        }
    }
    out
}

fn format_tool_action_message(tool_name: &str, arguments: &Value) -> String {
    let label = match tool_name {
        "CodeMode-executeCode" => "Running code",
        "CodeMode-searchApis" => "Searching APIs",
        "Memory-getSchema" => "Loading memory schema",
        "Memory-searchMemory" => "Searching memory",
        "Memory-storeFacts" => "Storing memory",
        "TaskGraph-createTask" => "Creating task",
        "TaskGraph-updateTask" => "Updating task",
        "TaskGraph-setTaskStatus" => "Updating task status",
        "TaskGraph-submitReview" => "Submitting review",
        "TaskGraph-approveReview" => "Approving review",
        "TaskGraph-requestReviewChanges" => "Requesting review changes",
        _ => "Running tool",
    };
    let pretty_args =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    format!("Action: {label}\n{}", pretty_args.trim())
}

#[cfg(test)]
mod tests {
    use super::{is_allowed_external_user, routing_key_for_chat_id};
    use borg_exec::TelegramSessionContext;

    #[test]
    fn routing_key_uses_chat_id() {
        let key = routing_key_for_chat_id(12345).expect("routing key");
        assert_eq!(key.as_str(), "telegram:conversation:12345");
    }

    #[test]
    fn allowlist_matches_numeric_id() {
        let mut ctx = TelegramSessionContext::default();
        ctx.set_chat(1, "private");
        ctx.set_last_message_refs(Some(10), None);
        ctx.upsert_participant(2_654_566, None, Some("Leo".to_string()), None);

        let allowed = vec!["2654566".to_string()];
        assert!(is_allowed_external_user(&allowed, &ctx));
    }

    #[test]
    fn allowlist_matches_username() {
        let mut ctx = TelegramSessionContext::default();
        ctx.set_chat(1, "private");
        ctx.set_last_message_refs(Some(11), None);
        ctx.upsert_participant(
            123,
            Some("leostera".to_string()),
            Some("Leo".to_string()),
            None,
        );

        let allowed = vec!["@leostera".to_string()];
        assert!(is_allowed_external_user(&allowed, &ctx));
    }
}
