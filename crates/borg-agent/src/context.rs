use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Agent, Message, ToolSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[async_trait]
pub trait ContextManager: Send + Sync {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow>;
}

const DEFAULT_MAX_CHARS: usize = 120_000;
const DEFAULT_KEEP_RECENT_MESSAGES: usize = 24;
const SUMMARY_MAX_ITEMS: usize = 24;
const SUMMARY_ITEM_MAX_CHARS: usize = 240;
const TELEGRAM_CONTEXT_PREFIX: &str = "TELEGRAM_CONTEXT_JSON: ";
const TELEGRAM_SESSION_CONTEXT_PREFIX: &str = "TELEGRAM_SESSION_CONTEXT_JSON: ";
const CONTEXT_METADATA_PREFIX: &str = "BORG_CONTEXT_METADATA_JSON: ";
const CHAR_TO_TOKEN_RATIO: usize = 4;

#[derive(Debug, Default)]
pub struct PassthroughContextManager;

#[async_trait]
impl ContextManager for PassthroughContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        let messages = with_context_metadata(
            messages.to_vec(),
            ContextMetadataPolicy::Passthrough,
            None,
        );
        Ok(ContextWindow {
            messages,
            tools: agent.tools.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CompactingContextManager {
    max_chars: usize,
    keep_recent_messages: usize,
}

impl Default for CompactingContextManager {
    fn default() -> Self {
        Self {
            max_chars: DEFAULT_MAX_CHARS,
            keep_recent_messages: DEFAULT_KEEP_RECENT_MESSAGES,
        }
    }
}

impl CompactingContextManager {
    pub fn new(max_chars: usize, keep_recent_messages: usize) -> Self {
        Self {
            max_chars: max_chars.max(1),
            keep_recent_messages: keep_recent_messages.max(1),
        }
    }
}

#[async_trait]
impl ContextManager for CompactingContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        let total_chars: usize = messages.iter().map(message_char_count).sum();
        if total_chars <= self.max_chars {
            let pinned = with_pinned_telegram_context(messages.to_vec());
            let pinned = with_context_metadata(
                pinned,
                ContextMetadataPolicy::Compacting {
                    max_chars: self.max_chars,
                    keep_recent_messages: self.keep_recent_messages,
                    was_compacted: false,
                },
                Some(self.max_chars),
            );
            return Ok(ContextWindow {
                messages: pinned,
                tools: agent.tools.clone(),
            });
        }

        let split_at = messages.len().saturating_sub(self.keep_recent_messages);
        let (older, recent) = messages.split_at(split_at);

        let mut compacted = Vec::new();
        let mut summary_items = Vec::new();
        for message in older {
            if matches!(message, Message::System { .. }) {
                compacted.push(message.clone());
                continue;
            }
            if summary_items.len() >= SUMMARY_MAX_ITEMS {
                continue;
            }
            if let Some(line) = summarize_message_line(message) {
                summary_items.push(line);
            }
        }

        if !summary_items.is_empty() {
            let mut summary = String::from(
                "Compacted conversation summary from older turns (for context continuity):",
            );
            for line in summary_items {
                summary.push('\n');
                summary.push_str("- ");
                summary.push_str(&line);
            }
            compacted.push(Message::System { content: summary });
        }

        compacted.extend_from_slice(recent);
        let compacted = with_pinned_telegram_context(compacted);
        let compacted = with_context_metadata(
            compacted,
            ContextMetadataPolicy::Compacting {
                max_chars: self.max_chars,
                keep_recent_messages: self.keep_recent_messages,
                was_compacted: true,
            },
            Some(self.max_chars),
        );
        Ok(ContextWindow {
            messages: compacted,
            tools: agent.tools.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SessionContextManager {
    inner: CompactingContextManager,
    telegram_session_context: Option<serde_json::Value>,
}

impl Default for SessionContextManager {
    fn default() -> Self {
        Self {
            inner: CompactingContextManager::default(),
            telegram_session_context: None,
        }
    }
}

impl SessionContextManager {
    pub fn for_telegram_session_context(telegram_session_context: serde_json::Value) -> Self {
        Self {
            inner: CompactingContextManager::default(),
            telegram_session_context: Some(telegram_session_context),
        }
    }
}

#[async_trait]
impl ContextManager for SessionContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        let context = self.inner.build_context(agent, messages).await?;
        let messages = with_pinned_telegram_session_context(
            context.messages,
            self.telegram_session_context.clone(),
        );
        Ok(ContextWindow {
            messages,
            tools: context.tools,
        })
    }
}

fn with_pinned_telegram_context(messages: Vec<Message>) -> Vec<Message> {
    let latest = messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::System { content } if content.starts_with(TELEGRAM_CONTEXT_PREFIX) => {
                Some(content.clone())
            }
            _ => None,
        });

    let Some(latest) = latest else {
        return messages;
    };

    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(Message::System { content: latest });
    for message in messages {
        match &message {
            Message::System { content } if content.starts_with(TELEGRAM_CONTEXT_PREFIX) => {}
            _ => out.push(message),
        }
    }
    out
}

fn with_pinned_telegram_session_context(
    messages: Vec<Message>,
    telegram_session_context: Option<serde_json::Value>,
) -> Vec<Message> {
    let Some(context) = telegram_session_context else {
        return messages;
    };
    let content = format!("{}{}", TELEGRAM_SESSION_CONTEXT_PREFIX, context);
    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(Message::System { content });
    for message in messages {
        match &message {
            Message::System { content } if content.starts_with(TELEGRAM_SESSION_CONTEXT_PREFIX) => {
            }
            _ => out.push(message),
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ContextMetadataPolicy {
    Passthrough,
    Compacting {
        max_chars: usize,
        keep_recent_messages: usize,
        was_compacted: bool,
    },
}

fn with_context_metadata(
    messages: Vec<Message>,
    autocompaction_policy: ContextMetadataPolicy,
    max_chars: Option<usize>,
) -> Vec<Message> {
    let base_messages: Vec<Message> = messages
        .into_iter()
        .filter(|message| match message {
            Message::System { content } => !content.starts_with(CONTEXT_METADATA_PREFIX),
            _ => true,
        })
        .collect();

    let total_chars: usize = base_messages.iter().map(message_char_count).sum();
    let current_tokens_used = total_chars / CHAR_TO_TOKEN_RATIO;
    let max_tokens = max_chars.map(|chars| chars / CHAR_TO_TOKEN_RATIO);
    let tokens_left = max_tokens.map(|max| max.saturating_sub(current_tokens_used));
    let now = Utc::now();
    let metadata = json!({
        "current_datetime_utc": now.to_rfc3339(),
        "current_unix_timestamp_secs": now.timestamp(),
        "current_tokens_used": current_tokens_used,
        "tokens_left": tokens_left,
        "autocompaction_policy": autocompaction_policy,
    });

    let mut out = Vec::with_capacity(base_messages.len() + 1);
    out.push(Message::System {
        content: format!("{}{}", CONTEXT_METADATA_PREFIX, metadata),
    });
    out.extend(base_messages);
    out
}

fn summarize_message_line(message: &Message) -> Option<String> {
    let mut line = match message {
        Message::System { content } => format!("System: {}", content),
        Message::User { content } => format!("User: {}", content),
        Message::Assistant { content } => format!("Assistant: {}", content),
        Message::ToolCall {
            name, arguments, ..
        } => format!("Tool call `{}` args {}", name, arguments),
        Message::ToolResult { name, content, .. } => {
            format!("Tool result `{}` {}", name, serde_json::to_string(content).ok()?)
        }
        Message::SessionEvent { name, .. } => format!("Session event `{}`", name),
    };

    if line.chars().count() > SUMMARY_ITEM_MAX_CHARS {
        line = line
            .chars()
            .take(SUMMARY_ITEM_MAX_CHARS.saturating_sub(3))
            .collect::<String>();
        line.push_str("...");
    }
    Some(line)
}

fn message_char_count(message: &Message) -> usize {
    match message {
        Message::System { content } => content.chars().count(),
        Message::User { content } => content.chars().count(),
        Message::Assistant { content } => content.chars().count(),
        Message::ToolCall {
            tool_call_id,
            name,
            arguments,
        } => tool_call_id.chars().count() + name.chars().count() + arguments.to_string().chars().count(),
        Message::ToolResult {
            tool_call_id,
            name,
            content,
        } => {
            tool_call_id.chars().count()
                + name.chars().count()
                + serde_json::to_string(content)
                    .map(|value| value.chars().count())
                    .unwrap_or(0)
        }
        Message::SessionEvent { name, payload } => {
            name.chars().count()
                + serde_json::to_string(payload)
                    .map(|value| value.chars().count())
                    .unwrap_or(0)
        }
    }
}
