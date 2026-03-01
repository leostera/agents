use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Uri, uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContextSnapshot {
    pub model: String,
    pub messages: Vec<Value>,
    pub tools: Vec<SessionToolSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    SessionStarted {
        session_id: Uri,
        agent_id: Uri,
    },
    SessionMessage {
        session_id: Uri,
        index: usize,
        message: Value,
    },
    ContextBuilt {
        session_id: Uri,
        context: SessionContextSnapshot,
    },
    LlmRequestSent {
        session_id: Uri,
        model: String,
        message_count: usize,
        tool_count: usize,
    },
    LlmResponseReceived {
        session_id: Uri,
        stop_reason: String,
        content_blocks: usize,
        tool_call_count: usize,
    },
    AgentIdle {
        session_id: Uri,
    },
    AgentToolCall {
        session_id: Uri,
        name: String,
        arguments: Value,
        output: Value,
    },
    AgentOutput {
        session_id: Uri,
        message: String,
    },
}

impl Event {
    pub fn session_id(&self) -> &Uri {
        match self {
            Self::SessionStarted { session_id, .. } => session_id,
            Self::SessionMessage { session_id, .. } => session_id,
            Self::ContextBuilt { session_id, .. } => session_id,
            Self::LlmRequestSent { session_id, .. } => session_id,
            Self::LlmResponseReceived { session_id, .. } => session_id,
            Self::AgentIdle { session_id } => session_id,
            Self::AgentToolCall { session_id, .. } => session_id,
            Self::AgentOutput { session_id, .. } => session_id,
        }
    }

    pub fn event_type(&self) -> Uri {
        match self {
            Self::SessionStarted { .. } => uri!("borg", "session", "started"),
            Self::SessionMessage { .. } => uri!("borg", "session", "message"),
            Self::ContextBuilt { .. } => uri!("borg", "session", "context_built"),
            Self::LlmRequestSent { .. } => uri!("borg", "llm", "request_sent"),
            Self::LlmResponseReceived { .. } => uri!("borg", "llm", "response_received"),
            Self::AgentIdle { .. } => uri!("borg", "agent", "idle"),
            Self::AgentToolCall { .. } => uri!("borg", "agent", "tool_call"),
            Self::AgentOutput { .. } => uri!("borg", "agent", "output"),
        }
    }
}
