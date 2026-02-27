mod agents;
mod core;
mod migrations;
mod policies;
mod ports;
mod providers;
mod sessions;
mod tasks;
mod users;
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionRecord {
    pub session_id: Uri,
    pub user_key: Uri,
    pub port: String,
    pub root_task_id: Uri,
    pub state: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionMessageRecord {
    pub message_id: Uri,
    pub session_id: Uri,
    pub message_index: i64,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserRecord {
    pub user_key: Uri,
    pub profile: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderRecord {
    pub provider: String,
    pub api_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyRecord {
    pub policy_id: Uri,
    pub policy: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyUseRecord {
    pub policy_id: Uri,
    pub entity_id: Uri,
    pub created_at: DateTime<Utc>,
}
