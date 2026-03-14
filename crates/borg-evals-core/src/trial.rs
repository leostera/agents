use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentTrial {
    pub transcript: Vec<RecordedEvent>,
    pub final_reply: String,
    pub tool_trace: Vec<RecordedToolCall>,
    #[serde(default)]
    pub metadata: Value,
}

impl AgentTrial {
    pub fn new(final_reply: impl Into<String>) -> Self {
        Self {
            transcript: Vec::new(),
            final_reply: final_reply.into(),
            tool_trace: Vec::new(),
            metadata: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedEvent {
    Message {
        role: RecordedMessageRole,
        content: String,
    },
    ToolCallRequested {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolExecutionCompleted {
        id: String,
        name: String,
        result: Value,
    },
    Completed {
        reply: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordedMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecordedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub result: Option<Value>,
    pub error: Option<String>,
}

