use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMessage {
    pub port: String,
    pub user_key: Uri,
    pub text: String,
    pub metadata: Value,
    pub session_id: Option<Uri>,
    pub agent_id: Option<Uri>,
    pub task_id: Option<String>,
    pub reply: Option<String>,
    pub tool_calls: Option<Vec<String>>,
    pub error: Option<String>,
}
