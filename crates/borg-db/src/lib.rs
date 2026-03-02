mod actor_bindings;
mod actors;
mod agents;
mod apps;
mod behaviors;
mod clockwork;
mod core;
mod llm_calls;
mod migrations;
mod policies;
mod ports;
mod providers;
mod sessions;
mod tool_calls;
mod users;
mod utils;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use borg_core::Uri;
pub use clockwork::{CreateClockworkJobInput, UpdateClockworkJobInput};

#[derive(Clone)]
pub struct BorgDb {
    conn: CompatConn,
}

#[derive(Clone)]
pub(crate) struct CompatConn {
    pool: SqlitePool,
}

impl CompatConn {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

impl BorgDb {
    pub fn pool(&self) -> &SqlitePool {
        self.conn.pool()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentSpecRecord {
    pub agent_id: Uri,
    pub name: String,
    pub enabled: bool,
    pub default_provider_id: Option<String>,
    pub model: String,
    pub system_prompt: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionRecord {
    pub session_id: Uri,
    pub users: Vec<Uri>,
    pub port: Uri,
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
    pub provider_kind: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub tokens_used: u64,
    pub last_used: Option<DateTime<Utc>>,
    pub default_text_model: Option<String>,
    pub default_audio_model: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortRecord {
    pub port_id: Uri,
    pub provider: String,
    pub port_name: String,
    pub enabled: bool,
    pub allows_guests: bool,
    pub default_agent_id: Option<Uri>,
    pub settings: Value,
    pub active_sessions: u64,
    pub updated_at: Option<DateTime<Utc>>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmCallRecord {
    pub call_id: String,
    pub provider: String,
    pub capability: String,
    pub model: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub status_reason: Option<String>,
    pub http_reason: Option<String>,
    pub error: Option<String>,
    pub latency_ms: Option<u64>,
    pub sent_at: DateTime<Utc>,
    pub received_at: Option<DateTime<Utc>>,
    pub request_json: Value,
    pub response_json: Value,
    pub response_body: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallRecord {
    pub call_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub arguments_json: Value,
    pub output_json: Value,
    pub success: bool,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub called_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppRecord {
    pub app_id: Uri,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub built_in: bool,
    pub source: String,
    pub auth_strategy: String,
    pub auth_config_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppCapabilityRecord {
    pub capability_id: Uri,
    pub app_id: Uri,
    pub name: String,
    pub hint: String,
    pub mode: String,
    pub instructions: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConnectionRecord {
    pub connection_id: Uri,
    pub app_id: Uri,
    pub owner_user_id: Option<Uri>,
    pub provider_account_id: Option<String>,
    pub external_user_id: Option<String>,
    pub status: String,
    pub connection_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppSecretRecord {
    pub secret_id: Uri,
    pub app_id: Uri,
    pub connection_id: Option<Uri>,
    pub key: String,
    pub value: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActorRecord {
    pub actor_id: Uri,
    pub name: String,
    pub system_prompt: String,
    pub default_behavior_id: Uri,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActorMailboxRecord {
    pub actor_message_id: Uri,
    pub actor_id: Uri,
    pub kind: String,
    pub session_id: Option<Uri>,
    pub payload: Value,
    pub status: String,
    pub reply_to_actor_id: Option<Uri>,
    pub reply_to_message_id: Option<Uri>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClockworkJobRecord {
    pub job_id: String,
    pub kind: String,
    pub status: String,
    pub target_actor_id: String,
    pub target_session_id: String,
    pub message_type: String,
    pub payload: Value,
    pub headers: Value,
    pub schedule_spec: Value,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClockworkJobRunRecord {
    pub run_id: String,
    pub job_id: String,
    pub scheduled_for: DateTime<Utc>,
    pub fired_at: DateTime<Utc>,
    pub target_actor_id: String,
    pub target_session_id: String,
    pub message_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BehaviorRecord {
    pub behavior_id: Uri,
    pub name: String,
    pub system_prompt: String,
    pub preferred_provider_id: Option<String>,
    pub required_capabilities_json: Value,
    pub session_turn_concurrency: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
