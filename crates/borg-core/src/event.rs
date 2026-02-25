use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{TaskKind, Uri, uri};

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
    TaskCreated {
        task_id: Uri,
        kind: TaskKind,
    },
    TaskClaimed {
        task_id: Uri,
        worker_id: Uri,
    },
    SessionStarted {
        task_id: Uri,
        session_id: Uri,
        agent_id: Uri,
    },
    SessionMessage {
        task_id: Uri,
        session_id: Uri,
        index: usize,
        message: Value,
    },
    ContextBuilt {
        task_id: Uri,
        session_id: Uri,
        context: SessionContextSnapshot,
    },
    LlmRequestSent {
        task_id: Uri,
        session_id: Uri,
        model: String,
        message_count: usize,
        tool_count: usize,
    },
    LlmResponseReceived {
        task_id: Uri,
        session_id: Uri,
        stop_reason: String,
        content_blocks: usize,
        tool_call_count: usize,
    },
    AgentIdle {
        task_id: Uri,
    },
    AgentToolCall {
        task_id: Uri,
        name: String,
        arguments: Value,
        output: Value,
    },
    AgentOutput {
        task_id: Uri,
        message: String,
    },
    TaskSucceeded {
        task_id: Uri,
        message: String,
    },
    TaskFailed {
        task_id: Uri,
        error: String,
    },
}

impl Event {
    pub fn task_id(&self) -> &Uri {
        match self {
            Self::TaskCreated { task_id, .. } => task_id,
            Self::TaskClaimed { task_id, .. } => task_id,
            Self::SessionStarted { task_id, .. } => task_id,
            Self::SessionMessage { task_id, .. } => task_id,
            Self::ContextBuilt { task_id, .. } => task_id,
            Self::LlmRequestSent { task_id, .. } => task_id,
            Self::LlmResponseReceived { task_id, .. } => task_id,
            Self::AgentIdle { task_id } => task_id,
            Self::AgentToolCall { task_id, .. } => task_id,
            Self::AgentOutput { task_id, .. } => task_id,
            Self::TaskSucceeded { task_id, .. } => task_id,
            Self::TaskFailed { task_id, .. } => task_id,
        }
    }

    pub fn event_type(&self) -> Uri {
        match self {
            Self::TaskCreated { .. } => uri!("borg", "task", "created"),
            Self::TaskClaimed { .. } => uri!("borg", "task", "claimed"),
            Self::SessionStarted { .. } => uri!("borg", "session", "started"),
            Self::SessionMessage { .. } => uri!("borg", "session", "message"),
            Self::ContextBuilt { .. } => uri!("borg", "session", "context_built"),
            Self::LlmRequestSent { .. } => uri!("borg", "llm", "request_sent"),
            Self::LlmResponseReceived { .. } => uri!("borg", "llm", "response_received"),
            Self::AgentIdle { .. } => uri!("borg", "agent", "idle"),
            Self::AgentToolCall { .. } => uri!("borg", "agent", "tool_call"),
            Self::AgentOutput { .. } => uri!("borg", "agent", "output"),
            Self::TaskSucceeded { .. } => uri!("borg", "task", "succeeded"),
            Self::TaskFailed { .. } => uri!("borg", "task", "failed"),
        }
    }
}
