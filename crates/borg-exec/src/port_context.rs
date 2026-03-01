use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub trait PortContext: std::fmt::Debug + Send + Sync {
    fn merge_message_metadata(&mut self, metadata: &Value) -> Result<()>;
    fn to_json(&self) -> Result<Value>;
    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Debug, Clone, Default)]
pub struct JsonPortContext {
    value: Value,
}

impl JsonPortContext {
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

impl PortContext for JsonPortContext {
    fn merge_message_metadata(&mut self, metadata: &Value) -> Result<()> {
        match (&mut self.value, metadata) {
            (Value::Object(target), Value::Object(incoming)) => {
                for (key, value) in incoming {
                    target.insert(key.clone(), value.clone());
                }
            }
            _ => {
                self.value = metadata.clone();
            }
        }
        Ok(())
    }

    fn to_json(&self) -> Result<Value> {
        Ok(self.value.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
