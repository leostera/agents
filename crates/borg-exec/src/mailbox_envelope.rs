use anyhow::Result;
use borg_core::Uri;
use serde::{Deserialize, Serialize};

use crate::message::{BorgCommand, BorgInput, BorgMessage};
use crate::port_context::PortContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ActorMailboxEnvelope {
    pub actor_id: String,
    pub user_id: String,
    pub port_context: PortContext,
    pub input: ActorMailboxInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(dead_code)]
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

#[allow(dead_code)]
impl ActorMailboxEnvelope {
    pub fn from_borg_message(msg: &BorgMessage) -> Self {
        Self {
            actor_id: msg.actor_id.to_string(),
            user_id: msg.user_id.to_string(),
            port_context: msg.port_context.clone(),
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

    pub fn to_borg_message(&self) -> Result<BorgMessage> {
        let actor_id = Uri::parse(&self.actor_id)?;
        let user_id = Uri::parse(&self.user_id)?;
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
            input,
            port_context: self.port_context.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_core::uri;

    #[test]
    fn mailbox_envelope_roundtrip_chat() {
        let port_ctx = serde_json::json!({"port":"unknown"});
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a1"),
            user_id: uri!("borg", "user", "u1"),
            input: BorgInput::Chat {
                text: "hello".to_string(),
            },
            port_context: serde_json::from_value(port_ctx.clone()).expect("ctx"),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = serde_json::to_value(&env).expect("to_json");
        let decoded = serde_json::from_value::<ActorMailboxEnvelope>(json)
            .expect("from_json")
            .to_borg_message()
            .expect("to_borg");

        assert_eq!(decoded.actor_id, msg.actor_id);
        assert_eq!(decoded.user_id, msg.user_id);
        assert_eq!(
            serde_json::to_value(&decoded.port_context).expect("ctx"),
            port_ctx
        );
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
            input: BorgInput::Command(BorgCommand::ModelSet {
                model: "gpt-5-mini".to_string(),
            }),
            port_context: serde_json::from_value(serde_json::json!({
                "port":"telegram",
                "chat_id":123,
                "chat_type":"private",
                "participants":{},
                "last_message_id":null,
                "last_thread_id":null
            }))
            .expect("ctx"),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = serde_json::to_value(&env).expect("to_json");
        let decoded = serde_json::from_value::<ActorMailboxEnvelope>(json)
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
            serde_json::to_value(&decoded.port_context).expect("ctx"),
            serde_json::json!({
                "port":"telegram",
                "chat_id":123,
                "chat_type":"private",
                "participants":{},
                "last_message_id":null,
                "last_thread_id":null
            })
        );
    }

    #[test]
    fn mailbox_envelope_roundtrip_audio() {
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a3"),
            user_id: uri!("borg", "user", "u3"),
            input: BorgInput::Audio {
                file_id: uri!("borg", "audio", "abc123"),
                mime_type: Some("audio/wav".to_string()),
                duration_ms: Some(1234),
                language_hint: Some("en".to_string()),
            },
            port_context: PortContext::Http(Default::default()),
        };

        let env = ActorMailboxEnvelope::from_borg_message(&msg);
        let json = serde_json::to_value(&env).expect("to_json");
        let decoded = serde_json::from_value::<ActorMailboxEnvelope>(json)
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
