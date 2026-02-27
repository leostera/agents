use borg_core::Uri;
use borg_exec::ToolCallSummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
// TODO(@leostera): PortMessage<Data> and replace text with `data: Data`, so we can keep data typed
// here until it has to be rendered into the transport (socket, http request, etc)
pub struct PortMessage {
    pub port: String,
    pub user_key: Uri,
    pub text: String,
    pub metadata: Value,
    pub session_id: Option<Uri>,
    pub agent_id: Option<Uri>,
    pub task_id: Option<String>,
    pub reply: Option<String>,
    pub tool_calls: Option<Vec<ToolCallSummary>>,
    pub error: Option<String>,
}
