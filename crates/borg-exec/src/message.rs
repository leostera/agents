use borg_agent::ToolResultData;
use borg_core::Uri;
use borg_llm::ReasoningEffort;
use serde::{Deserialize, Serialize};

use crate::port_context::PortContext;

pub type RuntimeToolCall = borg_agent::BorgToolCall;
pub type RuntimeToolResult = borg_agent::BorgToolResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BorgCommand {
    ModelShowCurrent,
    ModelSet { model: String },
    ReasoningShowCurrent,
    ReasoningSet { reasoning_effort: ReasoningEffort },
    ParticipantsList,
    ContextDump,
    CompactContext,
    ResetContext,
}

#[derive(Debug, Clone)]
pub enum BorgInput {
    Chat {
        text: String,
    },
    Audio {
        file_id: Uri,
        mime_type: Option<String>,
        duration_ms: Option<u64>,
        language_hint: Option<String>,
    },
    Command(BorgCommand),
}

#[derive(Debug, Clone)]
pub struct BorgMessage {
    pub actor_id: Uri,
    pub user_id: Uri,
    pub input: BorgInput,
    pub port_context: PortContext,
}

#[derive(Debug, Clone)]
pub struct ActorOutput<TToolCall, TToolResult> {
    pub actor_id: Uri,
    pub reply: Option<String>,
    pub tool_calls: Vec<ToolCallSummary<TToolCall, TToolResult>>,
    pub port_context: PortContext,
}

#[derive(Debug, Clone)]
pub struct ToolCallSummary<TToolCall, TToolResult> {
    pub tool_name: String,
    pub arguments: TToolCall,
    pub output: ToolResultData<TToolResult>,
}
