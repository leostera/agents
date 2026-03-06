use serde::{Deserialize, Serialize};

use crate::{Uri, uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolSchema<TParameters> {
    pub name: String,
    pub description: String,
    pub parameters: TParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContextSnapshot<TMessage, TParameters> {
    pub model: String,
    pub messages: Vec<TMessage>,
    pub tools: Vec<SessionToolSchema<TParameters>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event<TMessage, TParameters, TArguments, TOutput> {
    SessionStarted {
        session_id: Uri,
        actor_id: Uri,
    },
    SessionMessage {
        session_id: Uri,
        index: usize,
        message: TMessage,
    },
    ContextBuilt {
        session_id: Uri,
        context: SessionContextSnapshot<TMessage, TParameters>,
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
        arguments: TArguments,
        output: TOutput,
    },
    AgentOutput {
        session_id: Uri,
        message: String,
    },
}

impl<TMessage, TParameters, TArguments, TOutput> Event<TMessage, TParameters, TArguments, TOutput> {
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
