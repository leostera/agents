use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{TaskKind, Uri, uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    TaskCreated { task_id: Uri, kind: TaskKind },
    TaskClaimed { task_id: Uri, worker_id: Uri },
    AgentIdle { task_id: Uri },
    AgentToolCall {
        task_id: Uri,
        name: String,
        arguments: Value,
        output: Value,
    },
    AgentOutput { task_id: Uri, message: String },
    TaskSucceeded { task_id: Uri, message: String },
    TaskFailed { task_id: Uri, error: String },
}

impl Event {
    pub fn task_id(&self) -> &Uri {
        match self {
            Self::TaskCreated { task_id, .. } => task_id,
            Self::TaskClaimed { task_id, .. } => task_id,
            Self::AgentIdle { task_id } => task_id,
            Self::AgentToolCall { task_id, .. } => task_id,
            Self::AgentOutput { task_id, .. } => task_id,
            Self::TaskSucceeded { task_id, .. } => task_id,
            Self::TaskFailed { task_id, .. } => task_id,
        }
    }

    pub fn event_type(&self) -> Uri {
        match self {
            Self::TaskCreated { .. } => uri!("borg", "task", "created"),
            Self::TaskClaimed { .. } => uri!("borg", "task", "claimed"),
            Self::AgentIdle { .. } => uri!("borg", "agent", "idle"),
            Self::AgentToolCall { .. } => uri!("borg", "agent", "tool_call"),
            Self::AgentOutput { .. } => uri!("borg", "agent", "output"),
            Self::TaskSucceeded { .. } => uri!("borg", "task", "succeeded"),
            Self::TaskFailed { .. } => uri!("borg", "task", "failed"),
        }
    }
}
