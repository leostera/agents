use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_cmd::{CommandRegistry, CommandRequest};
use borg_core::Uri;
use borg_exec::{ExecEngine, UserMessage};
use serde_json::Value;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ParseMode};
use teloxide::utils::html;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

use crate::{Port, PortConfig, PortMessage};

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
    pub async fn run(self) -> Result<()> {
        let exec = self.exec.clone();
        let bot = self.bot.clone();

        teloxide::repl(bot, move |bot: Bot, message: Message| {
            let exec = exec.clone();
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
                        let response = match commands.run(text).await {
                            Ok(Some(value)) => value,
                            Ok(None) => String::new(),
                            Err(err) => format!("Command error: {err}"),
                        };
                        if !response.is_empty() {
                            bot.send_message(message.chat.id, truncate_telegram_message(response))
                                .await?;
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
                        for action in tool_calls {
                            let formatted = format_tool_action_message(&action);
                            bot.send_message(message.chat.id, truncate_telegram_message(formatted))
                                .parse_mode(ParseMode::Html)
                                .await?;
                        }
                    }

                    let reply = response
                        .reply
                        .unwrap_or_else(|| "Message processed, no reply generated.".to_string());
                    bot.send_message(message.chat.id, truncate_telegram_message(reply))
                        .await?;

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
}

struct TypingLoop {
    handle: JoinHandle<()>,
}

impl TypingLoop {
    fn start(bot: Bot, chat_id: ChatId) -> Self {
        let handle = tokio::spawn(async move {
            loop {
                let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;
                sleep(Duration::from_secs(TELEGRAM_TYPING_REFRESH_SECS)).await;
            }
        });
        Self { handle }
    }
}

impl Drop for TypingLoop {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn truncate_telegram_message(message: String) -> String {
    if message.chars().count() <= TELEGRAM_MESSAGE_LIMIT {
        return message;
    }

    let mut out = String::new();
    for ch in message
        .chars()
        .take(TELEGRAM_MESSAGE_LIMIT.saturating_sub(3))
    {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn telegram_port_info(message: &Message) -> String {
    let chat_id = message.chat.id.0;
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
    let session_uri = format!("borg:session:telegram_{chat_id}");
    format!("Port: telegram\nChat: {chat_type}\nSession: {session_uri}")
}

fn telegram_help_text() -> &'static str {
    "Available commands:\n/start - Show greeting\n/help - Show this help\n/compact - Compact current session context\n/port - Show current port info\n/participants - Show participants seen in this session\n/context - Dump the current context window"
}

fn build_telegram_command_registry(
    state: TelegramCommandState,
) -> Result<CommandRegistry<TelegramCommandState, String>> {
    CommandRegistry::build(state)
        .add_command("start", |req| async move { Ok(command_start(req)) })
        .add_command("help", |req| async move { Ok(command_help(req)) })
        .add_command("port", |req| async move { Ok(command_port(req)) })
        .add_command("compact", |req| async move { command_compact(req).await })
        .add_command("participants", |req| async move { command_participants(req).await })
        .add_command("context", |req| async move { command_context(req).await })
        .build()
}

fn command_start(req: CommandRequest<TelegramCommandState>) -> String {
    let _ = req;
    TELEGRAM_START_GREETING.to_string()
}

fn command_help(req: CommandRequest<TelegramCommandState>) -> String {
    let _ = req;
    telegram_help_text().to_string()
}

fn command_port(req: CommandRequest<TelegramCommandState>) -> String {
    telegram_port_info(&req.state.message)
}

async fn command_compact(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = telegram_session_id(&req.state.message)?;
    let kept = req.state.exec.compact_session(&session_id).await?;
    Ok(format!(
        "Compacted session. Kept {} context message(s).",
        kept
    ))
}

async fn command_participants(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = telegram_session_id(&req.state.message)?;
    let ctx = req
        .state
        .exec
        .get_port_session_context("telegram", &session_id)
        .await?;
    Ok(format_participants_message(
        ctx.as_ref(),
        &req.state.message,
    ))
}

async fn command_context(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = telegram_session_id(&req.state.message)?;
    let context = req.state.exec.context_window_for_session(&session_id).await?;
    let dump = serde_json::to_string_pretty(&context)?;
    Ok(dump)
}

fn telegram_session_id(message: &Message) -> Result<Uri> {
    Uri::from_parts(
        "borg",
        "session",
        Some(&format!("telegram_{}", message.chat.id.0)),
    )
    .map_err(Into::into)
}

fn format_participants_message(telegram_ctx: Option<&Value>, current_message: &Message) -> String {
    let mut participants = std::collections::BTreeSet::<String>::new();
    if let Some(current_sender) = telegram_sender_label(current_message) {
        participants.insert(current_sender);
    }

    if let Some(ctx) = telegram_ctx {
        if let Some(map) = ctx.get("participants").and_then(Value::as_object) {
            for participant in map.values() {
                let label = format_sender_label(
                    participant.get("id").and_then(Value::as_str),
                    participant.get("username").and_then(Value::as_str),
                    participant.get("first_name").and_then(Value::as_str),
                    participant.get("last_name").and_then(Value::as_str),
                );
                participants.insert(label);
            }
        }
    }

    if participants.is_empty() {
        return "Participants: none seen in context yet".to_string();
    }

    let mut out = String::from("Participants:");
    for participant in participants {
        out.push_str("\n- ");
        out.push_str(&participant);
    }
    out
}

fn telegram_sender_label(message: &Message) -> Option<String> {
    let sender = message.from.as_ref()?;
    Some(format_sender_label(
        Some(&sender.id.0.to_string()),
        sender.username.as_deref(),
        Some(sender.first_name.as_str()),
        sender.last_name.as_deref(),
    ))
}

fn format_sender_label(
    id: Option<&str>,
    username: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
) -> String {
    let id = id.unwrap_or("unknown");
    let username = username
        .map(|value| format!("@{value}"))
        .unwrap_or_else(|| "unknown".to_string());
    let full_name = format!(
        "{} {}",
        first_name.unwrap_or_default(),
        last_name.unwrap_or_default()
    )
    .trim()
    .to_string();
    if full_name.is_empty() {
        format!("{username} ({id})")
    } else {
        format!("{full_name} {username} ({id})")
    }
}

fn format_tool_action_message(action: &str) -> String {
    let Some((tool_name, raw_args)) = action.split_once(' ') else {
        return format!("<b>Action:</b> {}", html::escape(action));
    };
    let tool_label = humanize_tool_name(tool_name);
    let parsed_args = serde_json::from_str::<Value>(raw_args).ok();

    if tool_name == "execute" {
        if let Some(code) = parsed_args
            .as_ref()
            .and_then(|args| args.get("code"))
            .and_then(Value::as_str)
        {
            let title = infer_execute_action_title(code);
            return format!(
                "<b>Action:</b> {}\n<pre><code>{}</code></pre>",
                html::escape(title),
                html::escape(code.trim())
            );
        }
    }

    let pretty_args = parsed_args
        .as_ref()
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| raw_args.to_string());
    format!(
        "<b>Action:</b> {}\n<pre><code>{}</code></pre>",
        html::escape(tool_label),
        html::escape(pretty_args.trim())
    )
}

fn humanize_tool_name(tool_name: &str) -> &'static str {
    match tool_name {
        "execute" => "Running code",
        "memory__search" => "Searching memory",
        "memory__state_facts" => "Writing memory facts",
        _ => "Running tool",
    }
}

fn infer_execute_action_title(code: &str) -> &'static str {
    let lower = code.to_ascii_lowercase();
    if lower.contains("borg.os.ls") && lower.contains("movie") {
        return "Scanning for movies";
    }
    if lower.contains("borg.os.ls") {
        return "Scanning files";
    }
    if lower.contains("borg.memory.search") || lower.contains("memory__search") {
        return "Searching memory";
    }
    if lower.contains("borg.memory.statefacts") || lower.contains("memory__state_facts") {
        return "Saving to memory";
    }
    "Running code"
}

pub fn init_telegram_port(exec: ExecEngine, bot_token: impl Into<String>) -> Result<TelegramPort> {
    TelegramPort::init(PortConfig::Telegram {
        exec,
        bot_token: bot_token.into(),
    })
}
