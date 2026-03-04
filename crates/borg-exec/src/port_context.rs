use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramSessionContext {
    pub chat_id: i64,
    pub chat_type: String,
    pub participants: BTreeMap<String, TelegramParticipant>,
    pub last_message_id: Option<i64>,
    pub last_thread_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramParticipant {
    pub id: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

impl TelegramSessionContext {
    pub fn set_chat(&mut self, chat_id: i64, chat_type: impl Into<String>) {
        self.chat_id = chat_id;
        self.chat_type = chat_type.into();
    }

    pub fn set_last_message_refs(&mut self, message_id: Option<i64>, thread_id: Option<i64>) {
        self.last_message_id = message_id;
        self.last_thread_id = thread_id;
    }

    pub fn upsert_participant(
        &mut self,
        sender_id: u64,
        username: Option<String>,
        first_name: Option<String>,
        last_name: Option<String>,
    ) {
        let id = sender_id.to_string();
        self.participants.insert(
            id.clone(),
            TelegramParticipant {
                id,
                username,
                first_name,
                last_name,
            },
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordSessionContext {
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub message_id: u64,
    pub author_id: u64,
    pub author_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpSessionContext {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "port", rename_all = "snake_case")]
pub enum PortContext {
    Telegram(TelegramSessionContext),
    Discord(DiscordSessionContext),
    Http(HttpSessionContext),
    Unknown,
}

impl Default for PortContext {
    fn default() -> Self {
        Self::Unknown
    }
}

impl PortContext {
    pub fn as_telegram(&self) -> Option<&TelegramSessionContext> {
        match self {
            Self::Telegram(ctx) => Some(ctx),
            _ => None,
        }
    }

    pub fn as_discord(&self) -> Option<&DiscordSessionContext> {
        match self {
            Self::Discord(ctx) => Some(ctx),
            _ => None,
        }
    }
}
