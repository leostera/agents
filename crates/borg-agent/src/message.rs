use anyhow::Result;
use borg_core::Uri;
use serde::{Deserialize, Serialize};

use crate::ToolResultData;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message<TToolCall, TToolResult> {
    System {
        content: String,
    },
    User {
        content: String,
    },
    UserAudio {
        file_id: Uri,
        transcript: String,
        created_at: String,
    },
    Assistant {
        content: String,
    },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: TToolCall,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: ToolResultData<TToolResult>,
    },
    ActorEvent {
        name: String,
        payload: ActorEventPayload,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorEventPayload {
    Started {
        actor_id: Uri,
    },
    Finished {
        status: ActorRunStatus,
        reply: Option<String>,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorRunStatus {
    Completed,
    CompletedError,
    ActorError,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord<TToolCall, TToolResult> {
    pub tool_name: String,
    pub arguments: TToolCall,
    pub output: ToolResultData<TToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorRunOutput<TToolCall, TToolResult> {
    pub reply: String,
    pub tool_calls: Vec<ToolCallRecord<TToolCall, TToolResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorRunResult<T> {
    Completed(Result<T, String>),
    ActorError(String),
    Idle,
}
