//! Typed message payload envelope and variants.
//!
//! Every message payload stored in `payload_json` uses a stable typed
//! envelope with `kind` and `body`. Runtime code deserializes into these
//! typed values before processing.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, ToolCallId};

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// The canonical typed message payload envelope.
///
/// Serialized as `{"kind": "<discriminator>", "body": { ... }}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "body")]
pub enum MessagePayload {
    /// Plain text from a user or external source.
    #[serde(rename = "user_text")]
    UserText(UserTextPayload),

    /// Audio message from a user.
    #[serde(rename = "user_audio")]
    UserAudio(UserAudioPayload),

    /// A tool call request (persisted for history).
    #[serde(rename = "tool_call")]
    ToolCall(ToolCallPayload),

    /// A tool result returned from tool execution.
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultPayload),

    /// Actor-to-actor instruction message.
    #[serde(rename = "actor_instruction")]
    ActorInstruction(ActorInstructionPayload),

    /// Port-originated envelope (external system payload).
    #[serde(rename = "port_envelope")]
    PortEnvelope(PortEnvelopePayload),

    /// Final assistant message that terminates the current turn.
    #[serde(rename = "final_assistant_message")]
    FinalAssistantMessage(FinalAssistantPayload),

    /// Assistant intermediate message (non-final reply in history).
    #[serde(rename = "assistant_text")]
    AssistantText(AssistantTextPayload),

    /// System-level message (prompts, instructions).
    #[serde(rename = "system")]
    System(SystemPayload),
}

// ---------------------------------------------------------------------------
// Payload variant structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserTextPayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserAudioPayload {
    pub file_id: String,
    pub transcript: Option<String>,
    pub mime_type: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallPayload {
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultPayload {
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub result_json: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActorInstructionPayload {
    pub instruction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_actor_id: Option<ActorId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortEnvelopePayload {
    pub port_provider: String,
    pub external_payload_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinalAssistantPayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssistantTextPayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemPayload {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

impl MessagePayload {
    /// Quick constructor for user text.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::UserText(UserTextPayload { text: text.into() })
    }

    /// Quick constructor for assistant text (non-final).
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::AssistantText(AssistantTextPayload { text: text.into() })
    }

    /// Quick constructor for a final assistant message.
    pub fn final_assistant(text: impl Into<String>) -> Self {
        Self::FinalAssistantMessage(FinalAssistantPayload { text: text.into() })
    }

    /// Quick constructor for a system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self::System(SystemPayload { text: text.into() })
    }

    /// Quick constructor for an actor instruction.
    pub fn actor_instruction(instruction: impl Into<String>, sender: Option<ActorId>) -> Self {
        Self::ActorInstruction(ActorInstructionPayload {
            instruction: instruction.into(),
            sender_actor_id: sender,
        })
    }

    /// Serialize to a JSON string suitable for `payload_json` storage.
    pub fn to_json(&self) -> anyhow::Result<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    /// Deserialize from `payload_json` storage.
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        serde_json::from_str(json).map_err(Into::into)
    }

    /// Returns the kind discriminator as a string.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UserText(_) => "user_text",
            Self::UserAudio(_) => "user_audio",
            Self::ToolCall(_) => "tool_call",
            Self::ToolResult(_) => "tool_result",
            Self::ActorInstruction(_) => "actor_instruction",
            Self::PortEnvelope(_) => "port_envelope",
            Self::FinalAssistantMessage(_) => "final_assistant_message",
            Self::AssistantText(_) => "assistant_text",
            Self::System(_) => "system",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_text_roundtrip() {
        let payload = MessagePayload::user_text("hello world");
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
        assert_eq!(payload.kind(), "user_text");
    }

    #[test]
    fn final_assistant_roundtrip() {
        let payload = MessagePayload::final_assistant("all done");
        let json = payload.to_json().unwrap();
        assert!(json.contains("\"kind\":\"final_assistant_message\""));
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn tool_call_roundtrip() {
        let payload = MessagePayload::ToolCall(ToolCallPayload {
            tool_call_id: ToolCallId::from_id("call_001"),
            tool_name: "Patch-apply".to_string(),
            arguments_json: r#"{"patch":"..."}"#.to_string(),
        });
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn tool_result_roundtrip() {
        let payload = MessagePayload::ToolResult(ToolResultPayload {
            tool_call_id: ToolCallId::from_id("call_001"),
            tool_name: "Patch-apply".to_string(),
            result_json: r#"{"status":"ok"}"#.to_string(),
            is_error: false,
        });
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn actor_instruction_roundtrip() {
        let payload = MessagePayload::actor_instruction(
            "please review this code",
            Some(ActorId::parse("borg:actor:reviewer").unwrap()),
        );
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn port_envelope_roundtrip() {
        let payload = MessagePayload::PortEnvelope(PortEnvelopePayload {
            port_provider: "telegram".to_string(),
            external_payload_json: r#"{"text":"hi"}"#.to_string(),
        });
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn system_payload_roundtrip() {
        let payload = MessagePayload::system("you are a helpful assistant");
        let json = payload.to_json().unwrap();
        let parsed = MessagePayload::from_json(&json).unwrap();
        assert_eq!(payload, parsed);
    }

    #[test]
    fn kind_discriminator_values() {
        assert_eq!(MessagePayload::user_text("x").kind(), "user_text");
        assert_eq!(MessagePayload::assistant_text("x").kind(), "assistant_text");
        assert_eq!(
            MessagePayload::final_assistant("x").kind(),
            "final_assistant_message"
        );
        assert_eq!(MessagePayload::system("x").kind(), "system");
    }
}
