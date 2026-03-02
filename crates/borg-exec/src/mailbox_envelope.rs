use anyhow::{Result, anyhow};
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
    pub input: ActorMailboxInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorMailboxInput {
    Chat { text: String },
    Command { name: String },
}

impl ActorMailboxEnvelope {
    pub fn from_borg_message(msg: &BorgMessage) -> Self {
        Self {
            actor_id: msg.actor_id.to_string(),
            user_id: msg.user_id.to_string(),
            session_id: msg.session_id.to_string(),
            input: match &msg.input {
                BorgInput::Chat { text } => ActorMailboxInput::Chat { text: text.clone() },
                BorgInput::Command(command) => ActorMailboxInput::Command {
                    name: command_name(command).to_string(),
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
            ActorMailboxInput::Command { name } => {
                BorgInput::Command(parse_command(name)?)
            }
        };
        Ok(BorgMessage {
            actor_id,
            user_id,
            session_id,
            input,
            port_context: Arc::new(JsonPortContext::new(serde_json::json!({}))),
        })
    }
}

fn command_name(command: &BorgCommand) -> &'static str {
    match command {
        BorgCommand::ModelShowCurrent => "ModelShowCurrent",
        BorgCommand::ModelSet { .. } => "ModelSet",
        BorgCommand::ParticipantsList => "ParticipantsList",
        BorgCommand::ContextDump => "ContextDump",
        BorgCommand::CompactSession => "CompactSession",
        BorgCommand::ResetContext => "ResetContext",
    }
}

fn parse_command(name: &str) -> Result<BorgCommand> {
    match name {
        "ModelShowCurrent" => Ok(BorgCommand::ModelShowCurrent),
        "ParticipantsList" => Ok(BorgCommand::ParticipantsList),
        "ContextDump" => Ok(BorgCommand::ContextDump),
        "CompactSession" => Ok(BorgCommand::CompactSession),
        "ResetContext" => Ok(BorgCommand::ResetContext),
        "ModelSet" => Ok(BorgCommand::ModelSet {
            model: String::new(),
        }),
        other => Err(anyhow!("unsupported mailbox command name: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_core::uri;

    #[test]
    fn mailbox_envelope_roundtrip_chat() {
        let msg = BorgMessage {
            actor_id: uri!("devmode", "actor", "a1"),
            user_id: uri!("borg", "user", "u1"),
            session_id: uri!("borg", "session", "s1"),
            input: BorgInput::Chat {
                text: "hello".to_string(),
            },
            port_context: Arc::new(JsonPortContext::new(serde_json::json!({}))),
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
        match decoded.input {
            BorgInput::Chat { text } => assert_eq!(text, "hello"),
            _ => panic!("expected chat input"),
        }
    }
}
