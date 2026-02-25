use anyhow::Result;
use borg_core::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ToolResultData;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: ToolResultData,
    },
    SessionEvent {
        name: String,
        payload: SessionEventPayload,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEventPayload {
    Started {
        agent_id: Uri,
    },
    Finished {
        status: SessionEndStatus,
        reply: Option<String>,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEndStatus {
    Completed,
    CompletedError,
    SessionError,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: Value,
    pub output: ToolResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOutput {
    pub reply: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionResult<T> {
    Completed(Result<T, String>),
    SessionError(String),
    Idle,
}
