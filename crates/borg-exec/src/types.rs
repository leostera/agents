use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub user_id: Uri,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<Uri>,
    #[serde(default)]
    pub agent_id: Option<Uri>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub arguments: Value,
    pub output: Value,
}

impl ToolCallSummary {
    pub fn error_message(&self) -> Option<String> {
        self.output
            .get("Error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    pub fn is_error(&self) -> bool {
        self.error_message().is_some()
    }

    pub fn output_message(&self) -> String {
        if let Some(error) = self.error_message() {
            return error;
        }

        if let Some(execution) = self.output.get("Execution") {
            return serde_json::to_string_pretty(execution)
                .unwrap_or_else(|_| execution.to_string());
        }

        serde_json::to_string_pretty(&self.output).unwrap_or_else(|_| self.output.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionTurnOutput {
    pub session_id: Uri,
    pub reply: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallSummary>,
}
