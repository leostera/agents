use anyhow::{Context, Result};
use tracing::info;

use crate::BorgDb;

impl BorgDb {
    pub async fn migrate(&self) -> Result<()> {
        info!(target: "borg_db", "running control-plane/task migrations");
        self.conn
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS tasks (
                    task_id TEXT PRIMARY KEY,
                    parent_task_id TEXT,
                    status TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    claimed_by TEXT,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT
                );

                CREATE TABLE IF NOT EXISTS deps (
                    task_id TEXT NOT NULL,
                    depends_on_task_id TEXT NOT NULL,
                    PRIMARY KEY (task_id, depends_on_task_id)
                );

                CREATE TABLE IF NOT EXISTS task_events (
                    event_id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    ts TEXT NOT NULL,
                    type TEXT NOT NULL,
                    payload_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS sessions (
                    session_id TEXT PRIMARY KEY,
                    user_key TEXT NOT NULL,
                    port TEXT NOT NULL,
                    root_task_id TEXT NOT NULL,
                    state_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS session_messages (
                    message_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    message_index INTEGER NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    UNIQUE(session_id, message_index)
                );

                CREATE TABLE IF NOT EXISTS providers (
                    provider TEXT PRIMARY KEY,
                    api_key TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS port_settings (
                    port TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (port, key)
                );

                CREATE TABLE IF NOT EXISTS port_bindings (
                    port TEXT NOT NULL,
                    conversation_key TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    agent_id TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (port, conversation_key)
                );

                CREATE TABLE IF NOT EXISTS agent_specs (
                    agent_id TEXT PRIMARY KEY,
                    model TEXT NOT NULL,
                    system_prompt TEXT NOT NULL,
                    tools_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                INSERT OR IGNORE INTO providers(provider, api_key, created_at, updated_at)
                VALUES('kalosm', '', datetime('now'), datetime('now'));
                "#,
            )
            .await
            .context("failed to run db migrations")?;

        info!(target: "borg_db", "control-plane/task migrations completed");
        Ok(())
    }
}
