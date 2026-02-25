use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub user_key: Uri,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<Uri>,
    #[serde(default)]
    pub agent_id: Option<Uri>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionTurnOutput {
    pub session_id: Uri,
    pub reply: Option<String>,
}
