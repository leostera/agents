use borg_agent::ToolResultData;
use borg_core::Uri;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallSummary<TToolCall, TToolResult> {
    pub tool_name: String,
    pub arguments: TToolCall,
    pub output: ToolResultData<TToolResult>,
}

impl<TToolCall, TToolResult> ToolCallSummary<TToolCall, TToolResult>
where
    TToolCall: Serialize,
    TToolResult: Serialize,
{
    pub fn error_message(&self) -> Option<String> {
        match &self.output {
            ToolResultData::Error(message) => Some(message.clone()),
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
            ToolResultData::Ok(result) | ToolResultData::ByDesign(result) => {
                serde_json::to_string_pretty(result)
                    .unwrap_or_else(|_| "\"<invalid_result>\"".to_string())
            }
            ToolResultData::Error(message) => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActorTurnOutput<TToolCall, TToolResult> {
    pub actor_id: Uri,
    pub reply: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallSummary<TToolCall, TToolResult>>,
}
