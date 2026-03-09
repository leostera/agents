mod actor_bindings;
mod actors;
mod apps;
mod core;
mod devmode;
mod files;
mod llm_calls;
mod messages;
mod migrations;
mod ports;
mod providers;
mod schedule;
mod tool_calls;
mod utils;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use borg_core::{
    ActorId, CorrelationId, EndpointUri, LlmCallId, MessageId, MessagePayload, PortId,
    ProcessingState, ProviderId, ToolCallId, ToolCallStatus, Uri, WorkspaceId,
};
pub use schedule::{CreateScheduleJobInput, UpdateScheduleJobInput};

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

// ---------------------------------------------------------------------------
// RFD0033 Canonical Record Types
// ---------------------------------------------------------------------------

/// Canonical message record as stored in the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageRecord {
    pub message_id: MessageId,
    pub workspace_id: WorkspaceId,
    pub sender_id: EndpointUri,
    pub receiver_id: EndpointUri,
    pub payload: MessagePayload,
    pub conversation_id: Option<String>,
    pub in_reply_to_message_id: Option<MessageId>,
    pub correlation_id: Option<CorrelationId>,
    pub delivered_at: DateTime<Utc>,
    pub processing_state: ProcessingState,
    pub processed_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
}

/// Canonical tool call record as stored in the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallRecord {
    pub tool_call_id: ToolCallId,
    pub workspace_id: WorkspaceId,
    pub actor_id: ActorId,
    pub message_id: MessageId,
    pub tool_name: String,
    pub request_json: Value,
    pub result_json: Option<Value>,
    pub status: ToolCallStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// Canonical LLM call record as stored in the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmCallRecord {
    pub llm_call_id: LlmCallId,
    pub workspace_id: WorkspaceId,
    pub actor_id: ActorId,
    pub message_id: MessageId,
    pub provider_id: ProviderId,
    pub model: String,
    pub request_json: Value,
    pub response_json: Option<Value>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActorRecord {
    pub actor_id: ActorId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub system_prompt: String,
    pub actor_prompt: String,
    pub default_provider_id: Option<ProviderId>,
    pub model: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortRecord {
    pub port_id: PortId,
    pub workspace_id: WorkspaceId,
    pub provider: String,
    pub port_name: String,
    pub enabled: bool,
    pub allows_guests: bool,
    pub assigned_actor_id: Option<ActorId>,
    pub settings: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortBindingRecord {
    pub workspace_id: WorkspaceId,
    pub port_id: PortId,
    pub conversation_key: String,
    pub actor_id: ActorId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Remaining Core Types
// ---------------------------------------------------------------------------

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
    pub available_secrets: Vec<String>,
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
pub struct FileRecord {
    pub file_id: Uri,
    pub backend: String,
    pub storage_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha512: String,
    pub owner_uri: Option<Uri>,
    pub metadata_json: Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleJobRecord {
    pub job_id: String,
    pub kind: String,
    pub status: String,
    pub target_actor_id: String,
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
pub struct ScheduleJobRunRecord {
    pub run_id: String,
    pub job_id: String,
    pub scheduled_for: DateTime<Utc>,
    pub fired_at: DateTime<Utc>,
    pub target_actor_id: String,
    pub message_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DevModeProjectRecord {
    pub project_id: Uri,
    pub name: String,
    pub root_path: String,
    pub description: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DevModeSpecRecord {
    pub spec_id: Uri,
    pub project_id: Uri,
    pub title: String,
    pub body: String,
    pub status: String,
    pub root_task_uri: Option<Uri>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
