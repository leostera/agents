mod agents;
mod core;
mod migrations;
mod ports;
mod providers;
mod sessions;
mod tasks;
mod utils;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use turso::Connection;

use borg_core::{TaskKind, Uri};

#[derive(Clone)]
pub struct BorgDb {
    conn: Connection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewTask {
    pub kind: TaskKind,
    pub payload: Value,
    pub parent_task_id: Option<Uri>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentSpecRecord {
    pub agent_id: Uri,
    pub model: String,
    pub system_prompt: String,
    pub tools: Value,
    pub updated_at: DateTime<Utc>,
}
