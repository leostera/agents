use borg_agent::ToolResultData;
use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub arguments: Value,
    pub output: ToolResultData,
}

impl ToolCallSummary {
    pub fn error_message(&self) -> Option<String> {
        match &self.output {
            ToolResultData::Error { message } => Some(message.clone()),
            _ => None,
        }
    }

    pub fn is_error(&self) -> bool {
        self.error_message().is_some()
    }

    pub fn output_message(&self) -> String {
        if let Some(error) = self.error_message() {
            return error;
        }

        match &self.output {
            ToolResultData::Execution { result, .. } => {
                serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
            }
            ToolResultData::Text(text) => text.clone(),
            ToolResultData::Capabilities(capabilities) => {
                serde_json::to_string_pretty(capabilities).unwrap_or_else(|_| "[]".to_string())
            }
            ToolResultData::Error { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionTurnOutput {
    pub session_id: Uri,
    pub reply: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallSummary>,
}
