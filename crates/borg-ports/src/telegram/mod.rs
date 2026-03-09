use anyhow::Result;
use async_trait::async_trait;
use borg_agent::ToolResultData;
use borg_cmd::CommandRegistry;
use borg_core::{PortId, TelegramUserId, Uri};
use borg_exec::{
    ActorOutboundMessage, ActorOutput, BorgCommand, PortContext, ReasoningEffort, RuntimeToolCall,
    RuntimeToolResult, TelegramContext,
};
use serde::{Deserialize, Serialize};
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, warn};

use crate::message::PortInput;
use crate::output_format::{
    format_for_telegram_html, format_tool_action_message_for_telegram_html, split_message_by_limit,
};
use crate::{Port, PortConfig, PortMessage};

const TELEGRAM_USER_ID_PREFIX: &str = "telegram";
const TELEGRAM_CONVERSATION_PREFIX: &str = "telegram";
const TELEGRAM_MESSAGE_LIMIT: usize = 4000;
const TELEGRAM_START_GREETING: &str = "Borg is online. Send a message to start.";
const MODEL_COMMAND_USAGE: &str = "Usage: /model [model_name]";
const SETTINGS_COMMAND_USAGE: &str = "Usage: /settings reasoning [minimum|low|medium|high|xhigh]";

#[derive(Debug, Clone)]
enum TelegramCommandRoute {
    Local(String),
    LocalParticipants,
    Forward(BorgCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramConfig {
    bot_token: String,
    #[serde(default)]
    allowed_external_user_ids: Vec<String>,
}

#[derive(Clone)]
pub struct TelegramPort {
    port_id: PortId,
    port_name: String,
    allows_guests: bool,
    bot: Bot,
    telegram_config: TelegramConfig,
}

impl TelegramPort {
    fn port_message_from_text(&self, message: &Message) -> Option<PortMessage> {
        let text = message.text()?.to_string();
        self.port_message_from_input(message, PortInput::Chat { text })
    }

    fn port_message_from_input(&self, message: &Message, input: PortInput) -> Option<PortMessage> {
        let user_id = conversation_user_id(message)?;
        let conversation_key = conversation_routing_key(message)?;
        let ctx = telegram_context_from_message(message);
        if !self.allows_guests
            && !is_allowed_external_user(&self.telegram_config.allowed_external_user_ids, &ctx)
        {
            return None;
        }

        Some(PortMessage {
            port_id: self.port_id.clone().into_uri(),
            conversation_key,
            user_id,
            input,
            port_context: PortContext::Telegram(ctx),
        })
    }

    async fn send_output(
        &self,
        output: ActorOutput<RuntimeToolCall, RuntimeToolResult>,
    ) -> Result<()> {
        let Some(ctx) = output.port_context.as_telegram() else {
            return Ok(());
        };

        let chat_id = ChatId(ctx.chat_id);

        for call in output.tool_calls {
            let elapsed = tool_call_elapsed(&call.output);
            let body = format_tool_action_message_for_telegram_html(
                &call.tool_name,
                &call.arguments,
                elapsed,
            );
            self.send_html(chat_id, body).await?;
        }

        for outbound in output.outbound_messages {
            match outbound {
                ActorOutboundMessage::PortReply {
                    text, port_context, ..
                } => {
                    let Some(target_ctx) = port_context.as_telegram() else {
                        continue;
                    };
                    self.send_text(ChatId(target_ctx.chat_id), text).await?;
                }
            }
        }

        Ok(())
    }

    async fn send_text(&self, chat_id: ChatId, message: String) -> Result<()> {
        for chunk in split_message_by_limit(&message, TELEGRAM_MESSAGE_LIMIT) {
            let formatted = format_for_telegram_html(&chunk);
            self.bot
                .send_message(chat_id, formatted)
                .parse_mode(ParseMode::Html)
                .await?;
        }
        Ok(())
    }

    async fn send_html(&self, chat_id: ChatId, html: String) -> Result<()> {
        for chunk in split_message_by_limit(&html, TELEGRAM_MESSAGE_LIMIT) {
            self.bot
                .send_message(chat_id, chunk)
                .parse_mode(ParseMode::Html)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Port for TelegramPort {
    async fn new(port_config: PortConfig) -> Result<Self> {
        let telegram_config: TelegramConfig = serde_json::from_str(&port_config.settings_json)?;
        Ok(Self {
            port_id: port_config.port_id,
            port_name: port_config.port_name,
            allows_guests: matches!(port_config.privacy, crate::port::Privacy::Public),
            bot: Bot::new(telegram_config.bot_token.clone()),
            telegram_config,
        })
    }

    async fn run(
        self,
        inbound: Sender<PortMessage>,
        mut outbound: Receiver<ActorOutput<RuntimeToolCall, RuntimeToolResult>>,
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
        let command_registry = build_telegram_command_registry()?;

        teloxide::repl(bot, move |_bot: Bot, message: Message| {
            let inbound_port = inbound_port.clone();
            let inbound_tx = inbound_tx.clone();
            let command_registry = command_registry.clone();
            async move {
                let Some(text) = message.text() else {
                    return respond(());
                };

                if command_registry.is_command(text) {
                    if is_help_command(text) {
                        if let Err(err) = inbound_port
                            .send_text(message.chat.id, command_registry.help())
                            .await
                        {
                            warn!(
                                target: "borg_ports",
                                port_name = %inbound_port.port_name,
                                error = %err,
                                "failed to send local telegram help response"
                            );
                        }
                        return respond(());
                    }

                    match command_registry.run(text).await {
                        Ok(Some(TelegramCommandRoute::Local(reply))) => {
                            if let Err(err) = inbound_port.send_text(message.chat.id, reply).await {
                                warn!(
                                    target: "borg_ports",
                                    port_name = %inbound_port.port_name,
                                    error = %err,
                                    "failed to send local telegram command response"
                                );
                            }
                        }
                        Ok(Some(TelegramCommandRoute::LocalParticipants)) => {
                            let reply = format_participants_for_message(&message);
                            if let Err(err) = inbound_port.send_text(message.chat.id, reply).await {
                                warn!(
                                    target: "borg_ports",
                                    port_name = %inbound_port.port_name,
                                    error = %err,
                                    "failed to send local participants response"
                                );
                            }
                        }
                        Ok(Some(TelegramCommandRoute::Forward(command))) => {
                            if let Some(port_message) = inbound_port
                                .port_message_from_input(&message, PortInput::Command(command))
                                && inbound_tx.send(port_message).await.is_err()
                            {
                                warn!(
                                    target: "borg_ports",
                                    port_name = %inbound_port.port_name,
                                    "port inbound channel closed"
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(err) => {
                            let _ = inbound_port
                                .send_text(message.chat.id, format!("Command error: {err}"))
                                .await;
                        }
                    }
                    return respond(());
                }

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

fn build_telegram_command_registry() -> Result<CommandRegistry<(), TelegramCommandRoute>> {
    CommandRegistry::build(())
        .add_command("start", |_req| async move {
            Ok(TelegramCommandRoute::Local(
                TELEGRAM_START_GREETING.to_string(),
            ))
        })
        .add_command("model", |req| async move {
            match parse_model_command_action(&req.args) {
                ModelCommandAction::Show => {
                    Ok(TelegramCommandRoute::Forward(BorgCommand::ModelShowCurrent))
                }
                ModelCommandAction::Set(model) => {
                    Ok(TelegramCommandRoute::Forward(BorgCommand::ModelSet {
                        model,
                    }))
                }
                ModelCommandAction::Usage => {
                    Ok(TelegramCommandRoute::Local(MODEL_COMMAND_USAGE.to_string()))
                }
            }
        })
        .add_command("settings", |req| async move {
            match parse_settings_command_action(&req.args) {
                SettingsCommandAction::ReasoningShow => Ok(TelegramCommandRoute::Forward(
                    BorgCommand::ReasoningShowCurrent,
                )),
                SettingsCommandAction::ReasoningSet(reasoning_effort) => {
                    Ok(TelegramCommandRoute::Forward(BorgCommand::ReasoningSet {
                        reasoning_effort,
                    }))
                }
                SettingsCommandAction::Usage => Ok(TelegramCommandRoute::Local(
                    SETTINGS_COMMAND_USAGE.to_string(),
                )),
            }
        })
        .add_command("participants", |_req| async move {
            Ok(TelegramCommandRoute::LocalParticipants)
        })
        .add_command("context", |_req| async move {
            Ok(TelegramCommandRoute::Forward(BorgCommand::ContextDump))
        })
        .add_command("reset", |_req| async move {
            Ok(TelegramCommandRoute::Forward(BorgCommand::ResetContext))
        })
        .add_command("compact", |_req| async move {
            Ok(TelegramCommandRoute::Forward(BorgCommand::CompactContext))
        })
        .build()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelCommandAction {
    Show,
    Set(String),
    Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingsCommandAction {
    ReasoningShow,
    ReasoningSet(ReasoningEffort),
    Usage,
}

fn parse_model_command_action(args: &[String]) -> ModelCommandAction {
    match args {
        [] => ModelCommandAction::Show,
        [model] if !model.trim().is_empty() => ModelCommandAction::Set(model.trim().to_string()),
        [..] => ModelCommandAction::Usage,
    }
}

fn parse_settings_command_action(args: &[String]) -> SettingsCommandAction {
    match args {
        [setting] if setting.eq_ignore_ascii_case("reasoning") => {
            SettingsCommandAction::ReasoningShow
        }
        [setting, level] if setting.eq_ignore_ascii_case("reasoning") => {
            let normalized = level.trim().to_ascii_lowercase();
            let effort = match normalized.as_str() {
                "minimum" | "minimal" => Some(ReasoningEffort::Minimal),
                "low" => Some(ReasoningEffort::Low),
                "medium" => Some(ReasoningEffort::Medium),
                "high" => Some(ReasoningEffort::High),
                "xhigh" => Some(ReasoningEffort::XHigh),
                _ => None,
            };
            match effort {
                Some(reasoning_effort) => SettingsCommandAction::ReasoningSet(reasoning_effort),
                None => SettingsCommandAction::Usage,
            }
        }
        _ => SettingsCommandAction::Usage,
    }
}

fn is_help_command(input: &str) -> bool {
    let token = input.split_whitespace().next().unwrap_or_default();
    if !token.starts_with('/') {
        return false;
    }
    let command = token
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or("");
    command.eq_ignore_ascii_case("help")
}

fn format_participants_for_message(message: &Message) -> String {
    let ctx = telegram_context_from_message(message);
    if ctx.participants.is_empty() {
        return "No participants available for this conversation.".to_string();
    }

    let mut lines = Vec::new();
    lines.push(format!("Chat {} ({})", ctx.chat_id, ctx.chat_type));
    lines.push("Participants:".to_string());
    for participant in ctx.participants.values() {
        let display = participant
            .username
            .as_ref()
            .map(|username| format!("@{username}"))
            .or_else(|| participant.first_name.clone())
            .unwrap_or_else(|| participant.id.clone());
        lines.push(format!("- {} [{}]", display, participant.id));
    }
    lines.join("\n")
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

fn telegram_context_from_message(message: &Message) -> TelegramContext {
    let mut ctx = TelegramContext::default();
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

fn is_allowed_external_user(allowed_external_user_ids: &[String], ctx: &TelegramContext) -> bool {
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

fn telegram_candidates(ctx: &TelegramContext) -> Vec<TelegramUserId> {
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

fn tool_call_elapsed(output: &ToolResultData<RuntimeToolResult>) -> Option<std::time::Duration> {
    match output {
        ToolResultData::Ok(result) | ToolResultData::ByDesign(result) => result
            .to_value()
            .ok()
            .and_then(|value| value.get("duration_ms").and_then(|value| value.as_u64()))
            .map(std::time::Duration::from_millis),
        ToolResultData::Error(_) => None,
    }
}
