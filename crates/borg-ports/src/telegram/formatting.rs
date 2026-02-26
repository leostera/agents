use borg_exec::ToolCallSummary;
use serde_json::Value;
use teloxide::prelude::*;

use super::{TELEGRAM_MESSAGE_LIMIT, TelegramPort};

impl TelegramPort {
    pub(super) async fn send_text(
        &self,
        chat_id: ChatId,
        message: String,
    ) -> std::result::Result<(), teloxide::RequestError> {
        for chunk in Self::split_message(&message) {
            self.bot.send_message(chat_id, chunk).await?;
        }
        Ok(())
    }

    fn split_message(message: &str) -> Vec<String> {
        if message.is_empty() {
            return Vec::new();
        }
        if message.chars().count() <= TELEGRAM_MESSAGE_LIMIT {
            return vec![message.to_string()];
        }

        let mut remaining = message;
        let mut chunks = Vec::new();

        while !remaining.is_empty() {
            if remaining.chars().count() <= TELEGRAM_MESSAGE_LIMIT {
                chunks.push(remaining.to_string());
                break;
            }

            let split_byte = remaining
                .char_indices()
                .nth(TELEGRAM_MESSAGE_LIMIT)
                .map(|(idx, _)| idx)
                .unwrap_or(remaining.len());

            let window = &remaining[..split_byte];
            let preferred = window.rfind('\n').unwrap_or(split_byte);
            let take = if preferred == 0 { split_byte } else { preferred };
            let (head, tail) = remaining.split_at(take);
            chunks.push(head.trim_end().to_string());
            remaining = tail.trim_start_matches('\n');
        }

        chunks.retain(|part| !part.is_empty());
        chunks
    }

    pub(super) fn port_info(message: &Message) -> String {
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

    pub(super) fn format_participants_message(
        telegram_ctx: Option<&Value>,
        current_message: &Message,
    ) -> String {
        let mut participants = std::collections::BTreeSet::<String>::new();
        if let Some(current_sender) = Self::sender_label(current_message) {
            participants.insert(current_sender);
        }

        if let Some(ctx) = telegram_ctx {
            if let Some(map) = ctx.get("participants").and_then(Value::as_object) {
                for participant in map.values() {
                    let label = Self::format_sender_label(
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
        if let Some(member_count) = telegram_ctx
            .and_then(|ctx| ctx.get("member_count"))
            .and_then(Value::as_i64)
        {
            out.push_str(&format!(
                " (known {} / reported {})",
                participants.len(),
                member_count
            ));
        }
        for participant in participants {
            out.push_str("\n- ");
            out.push_str(&participant);
        }
        out
    }

    fn sender_label(message: &Message) -> Option<String> {
        let sender = message.from.as_ref()?;
        Some(Self::format_sender_label(
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
        let username = username.map(|value| format!("@{value}"));
        let full_name = format!(
            "{} {}",
            first_name.unwrap_or_default(),
            last_name.unwrap_or_default()
        )
        .trim()
        .to_string();
        match (full_name.is_empty(), username) {
            (false, Some(username)) => format!("{full_name} {username} ({id})"),
            (false, None) => format!("{full_name} ({id})"),
            (true, Some(username)) => format!("{username} ({id})"),
            (true, None) => format!("({id})"),
        }
    }

    pub(super) fn format_tool_action_message(&self, call: &ToolCallSummary) -> String {
        let tool_name = call.tool_name.as_str();
        let raw_args = call.arguments.to_string();
        let tool_label = Self::humanize_tool_name(tool_name);
        let parsed_args = Some(call.arguments.clone());

        if tool_name == "execute" {
            let hinted_title = parsed_args
                .as_ref()
                .and_then(|args| args.get("hint"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(title) = hinted_title else {
                return format!(
                    "Action: {}\n{}",
                    "Invalid execute call: missing required `hint`",
                    parsed_args
                        .as_ref()
                        .and_then(|args| args.get("code"))
                        .and_then(Value::as_str)
                        .unwrap_or(raw_args.as_str())
                        .trim()
                );
            };
            if let Some(code) = parsed_args
                .as_ref()
                .and_then(|args| args.get("code"))
                .and_then(Value::as_str)
            {
                return format!("Action: {title}\n{}", code.trim());
            }
            return format!("Action: {title}");
        }

        let pretty_args = parsed_args
            .as_ref()
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .unwrap_or_else(|| raw_args.to_string());
        format!("Action: {tool_label}\n{}", pretty_args.trim())
    }

    fn humanize_tool_name(tool_name: &str) -> &'static str {
        match tool_name {
            "execute" => "Running code",
            "memory__search" => "Searching memory",
            "memory__state_facts" => "Writing memory facts",
            _ => "Running tool",
        }
    }
}
