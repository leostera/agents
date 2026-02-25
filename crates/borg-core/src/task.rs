use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{trace, warn};
use crate::Uri;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Blocked,
    Succeeded,
    Failed,
    Canceled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        let parsed = match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "blocked" => Self::Blocked,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "canceled" => Self::Canceled,
            _ => {
                warn!(
                    target: "borg_core",
                    task_status = value,
                    "unknown task status value"
                );
                return None;
            }
        };
        trace!(
            target: "borg_core",
            task_status = value,
            "parsed task status"
        );
        Some(parsed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    UserMessage,
    AgentAction,
    ToolCall,
    System,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserMessage => "user_message",
            Self::AgentAction => "agent_action",
            Self::ToolCall => "tool_call",
            Self::System => "system",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        let parsed = match value {
            "user_message" => Self::UserMessage,
            "agent_action" => Self::AgentAction,
            "tool_call" => Self::ToolCall,
            "system" => Self::System,
            _ => {
                warn!(
                    target: "borg_core",
                    task_kind = value,
                    "unknown task kind value"
                );
                return None;
            }
        };
        trace!(target: "borg_core", task_kind = value, "parsed task kind");
        Some(parsed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: Uri,
    pub parent_task_id: Option<Uri>,
    pub status: TaskStatus,
    pub kind: TaskKind,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub claimed_by: Option<Uri>,
    pub attempts: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub event_id: Uri,
    pub task_id: Uri,
    pub ts: DateTime<Utc>,
    pub event_type: Uri,
    pub payload: Value,
}
