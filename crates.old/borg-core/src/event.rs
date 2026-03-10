use serde::{Deserialize, Serialize};

use crate::{Uri, uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorToolSchema<TParameters> {
    pub name: String,
    pub description: String,
    pub parameters: TParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContextSnapshot<TMessage, TParameters> {
    pub model: String,
    pub messages: Vec<TMessage>,
    pub tools: Vec<ActorToolSchema<TParameters>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event<TMessage, TParameters, TArguments, TOutput> {
    ActorStarted {
        actor_id: Uri,
    },
    ActorMessage {
        actor_id: Uri,
        index: usize,
        message: TMessage,
    },
    ContextBuilt {
        actor_id: Uri,
        context: ActorContextSnapshot<TMessage, TParameters>,
    },
    LlmRequestSent {
        actor_id: Uri,
        model: String,
        message_count: usize,
        tool_count: usize,
    },
    LlmResponseReceived {
        actor_id: Uri,
        stop_reason: String,
        content_blocks: usize,
        tool_call_count: usize,
    },
    AgentIdle {
        actor_id: Uri,
    },
    AgentToolCall {
        actor_id: Uri,
        name: String,
        arguments: TArguments,
        output: TOutput,
    },
    AgentOutput {
        actor_id: Uri,
        message: String,
    },
}

impl<TMessage, TParameters, TArguments, TOutput> Event<TMessage, TParameters, TArguments, TOutput> {
    pub fn actor_id(&self) -> &Uri {
        match self {
            Self::ActorStarted { actor_id, .. } => actor_id,
            Self::ActorMessage { actor_id, .. } => actor_id,
            Self::ContextBuilt { actor_id, .. } => actor_id,
            Self::LlmRequestSent { actor_id, .. } => actor_id,
            Self::LlmResponseReceived { actor_id, .. } => actor_id,
            Self::AgentIdle { actor_id } => actor_id,
            Self::AgentToolCall { actor_id, .. } => actor_id,
            Self::AgentOutput { actor_id, .. } => actor_id,
        }
    }

    pub fn event_type(&self) -> Uri {
        match self {
            Self::ActorStarted { .. } => uri!("borg", "actor", "started"),
            Self::ActorMessage { .. } => uri!("borg", "actor", "message"),
            Self::ContextBuilt { .. } => uri!("borg", "actor", "context_built"),
            Self::LlmRequestSent { .. } => uri!("borg", "llm", "request_sent"),
            Self::LlmResponseReceived { .. } => uri!("borg", "llm", "response_received"),
            Self::AgentIdle { .. } => uri!("borg", "agent", "idle"),
            Self::AgentToolCall { .. } => uri!("borg", "agent", "tool_call"),
            Self::AgentOutput { .. } => uri!("borg", "agent", "output"),
        }
    }
}
