use anyhow::Result;
use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::message::{BorgCommand, BorgInput, BorgMessage};
use crate::port_context::JsonPortContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorMailboxEnvelope {
    pub actor_id: String,
    pub user_id: String,
    pub session_id: String,
    pub port_context: Value,
    pub input: ActorMailboxInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorMailboxInput {
    Chat {
        text: String,
    },
    Audio {
        file_id: String,
        mime_type: Option<String>,
        duration_ms: Option<u64>,
        language_hint: Option<String>,
    },
    Command {
        command: BorgCommand,
    },
}

impl ActorMailboxEnvelope {
    pub fn from_borg_message(msg: &BorgMessage) -> Self {
        let port_context = msg
            .port_context
            .to_json()
            .unwrap_or_else(|_| serde_json::json!({}));
        Self {
            actor_id: msg.actor_id.to_string(),
            user_id: msg.user_id.to_string(),
            session_id: msg.session_id.to_string(),
            port_context,
            input: match &msg.input {
                BorgInput::Chat { text } => ActorMailboxInput::Chat { text: text.clone() },
                BorgInput::Audio {
                    file_id,
                    mime_type,
                    duration_ms,
                    language_hint,
                } => ActorMailboxInput::Audio {
                    file_id: file_id.to_string(),
                    mime_type: mime_type.clone(),
                    duration_ms: *duration_ms,
                    language_hint: language_hint.clone(),
                },
                BorgInput::Command(command) => ActorMailboxInput::Command {
                    command: command.clone(),
                },
            },
        }
    }

    pub fn to_json(&self) -> Result<Value> {
        Ok(serde_json::to_value(self)?)
    }

    pub fn from_json(value: &Value) -> Result<Self> {
        Ok(serde_json::from_value(value.clone())?)
    }

    pub fn to_borg_message(&self) -> Result<BorgMessage> {
        let actor_id = Uri::parse(&self.actor_id)?;
        let user_id = Uri::parse(&self.user_id)?;
        let session_id = Uri::parse(&self.session_id)?;
        let input = match &self.input {
            ActorMailboxInput::Chat { text } => BorgInput::Chat { text: text.clone() },
            ActorMailboxInput::Audio {
                file_id,
                mime_type,
                duration_ms,
                language_hint,
            } => BorgInput::Audio {
                file_id: Uri::parse(file_id)?,
                mime_type: mime_type.clone(),
                duration_ms: *duration_ms,
                language_hint: language_hint.clone(),
            },
            ActorMailboxInput::Command { command } => BorgInput::Command(command.clone()),
        };
        Ok(BorgMessage {
            actor_id,
            user_id,
            session_id,
            input,
            port_context: Arc::new(JsonPortContext::new(self.port_context.clone())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_core::uri;

    #[test]
    fn mailbox_envelope_roundtrip_chat() {
        let port_ctx = serde_json::json!({"port":"test"});
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a1"),
            user_id: uri!("borg", "user", "u1"),
            session_id: uri!("borg", "session", "s1"),
            input: BorgInput::Chat {
                text: "hello".to_string(),
            },
            port_context: Arc::new(JsonPortContext::new(port_ctx.clone())),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = env.to_json().expect("to_json");
        let decoded = ActorMailboxEnvelope::from_json(&json)
            .expect("from_json")
            .to_borg_message()
            .expect("to_borg");

        assert_eq!(decoded.actor_id, msg.actor_id);
        assert_eq!(decoded.user_id, msg.user_id);
        assert_eq!(decoded.session_id, msg.session_id);
        assert_eq!(decoded.port_context.to_json().expect("ctx"), port_ctx);
        match decoded.input {
            BorgInput::Chat { text } => assert_eq!(text, "hello"),
            _ => panic!("expected chat input"),
        }
    }

    #[test]
    fn mailbox_envelope_roundtrip_model_set() {
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a2"),
            user_id: uri!("borg", "user", "u2"),
            session_id: uri!("borg", "session", "s2"),
            input: BorgInput::Command(BorgCommand::ModelSet {
                model: "gpt-5-mini".to_string(),
            }),
            port_context: Arc::new(JsonPortContext::new(serde_json::json!({"chat_id":123}))),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = env.to_json().expect("to_json");
        let decoded = ActorMailboxEnvelope::from_json(&json)
            .expect("from_json")
            .to_borg_message()
            .expect("to_borg");

        match decoded.input {
            BorgInput::Command(BorgCommand::ModelSet { model }) => {
                assert_eq!(model, "gpt-5-mini")
            }
            _ => panic!("expected model-set command"),
        }
        assert_eq!(
            decoded.port_context.to_json().expect("ctx"),
            serde_json::json!({"chat_id":123})
        );
    }

    #[test]
    fn mailbox_envelope_roundtrip_audio() {
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a3"),
            user_id: uri!("borg", "user", "u3"),
            session_id: uri!("borg", "session", "s3"),
            input: BorgInput::Audio {
                file_id: uri!("borg", "audio", "abc123"),
                mime_type: Some("audio/wav".to_string()),
                duration_ms: Some(1234),
                language_hint: Some("en".to_string()),
            },
            port_context: Arc::new(JsonPortContext::new(serde_json::json!({"port":"http"}))),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = env.to_json().expect("to_json");
        let decoded = ActorMailboxEnvelope::from_json(&json)
            .expect("from_json")
            .to_borg_message()
            .expect("to_borg");

        match decoded.input {
            BorgInput::Audio {
                file_id,
                mime_type,
                duration_ms,
                language_hint,
            } => {
                assert_eq!(file_id.as_str(), "borg:audio:abc123");
                assert_eq!(mime_type.as_deref(), Some("audio/wav"));
                assert_eq!(duration_ms, Some(1234));
                assert_eq!(language_hint.as_deref(), Some("en"));
            }
            _ => panic!("expected audio input"),
        }
    }
}
