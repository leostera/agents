use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Default)]
pub struct PassthroughContextManager;

#[async_trait]
impl ContextManager for PassthroughContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        Ok(ContextWindow {
            messages: messages.to_vec(),
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
        Ok(ContextWindow {
            messages: compacted,
            tools: agent.tools.clone(),
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
