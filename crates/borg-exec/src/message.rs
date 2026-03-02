use borg_core::Uri;
use std::sync::Arc;

use crate::port_context::PortContext;

#[derive(Debug, Clone)]
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
    Chat { text: String },
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
pub struct SessionOutput {
    pub session_id: Uri,
    pub reply: Option<String>,
    pub tool_calls: Vec<ToolCallSummary>,
    pub port_context: Arc<dyn PortContext>,
}

#[derive(Debug, Clone)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub output: serde_json::Value,
}
