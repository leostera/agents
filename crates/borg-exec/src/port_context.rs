use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub trait PortContext {
    fn merge_message_metadata(&mut self, metadata: &Value) -> Result<()>;
    fn to_json(&self) -> Result<Value>;
}

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
    pub fn from_json(value: Value) -> Result<Self> {
        serde_json::from_value(value)
            .map_err(|err| anyhow!("invalid telegram session context: {err}"))
    }
}

impl PortContext for TelegramSessionContext {
    fn merge_message_metadata(&mut self, metadata: &Value) -> Result<()> {
        let obj = metadata
            .as_object()
            .ok_or_else(|| anyhow!("telegram metadata must be an object"))?;
        let chat_id = obj
            .get("chat_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("telegram metadata missing chat_id"))?;
        let chat_type = obj
            .get("chat_type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("telegram metadata missing chat_type"))?
            .to_string();
        self.chat_id = chat_id;
        self.chat_type = chat_type;
        self.last_message_id = obj.get("message_id").and_then(Value::as_i64);
        self.last_thread_id = obj.get("thread_id").and_then(Value::as_i64);

        let sender_id = obj
            .get("sender_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("telegram metadata missing sender_id"))?
            .to_string();
        let participant = TelegramParticipant {
            id: sender_id.clone(),
            username: obj
                .get("sender_username")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            first_name: obj
                .get("sender_first_name")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            last_name: obj
                .get("sender_last_name")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        };
        self.participants.insert(sender_id, participant);
        Ok(())
    }

    fn to_json(&self) -> Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
}
