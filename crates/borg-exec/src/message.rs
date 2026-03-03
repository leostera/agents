use borg_agent::ToolResultData;
use borg_core::Uri;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::port_context::PortContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BorgCommand {
    ModelShowCurrent,
    ModelSet { model: String },
    ParticipantsList,
    ContextDump,
    CompactSession,
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
    pub session_id: Uri,
    pub input: BorgInput,
    pub port_context: Arc<dyn PortContext>,
}

#[derive(Debug, Clone)]
pub struct SessionOutput<TToolCall, TToolResult> {
    pub session_id: Uri,
    pub reply: Option<String>,
    pub tool_calls: Vec<ToolCallSummary<TToolCall, TToolResult>>,
    pub port_context: Arc<dyn PortContext>,
}

#[derive(Debug, Clone)]
pub struct ToolCallSummary<TToolCall, TToolResult> {
    pub tool_name: String,
    pub arguments: TToolCall,
    pub output: ToolResultData<TToolResult>,
}
