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
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
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
                    ts TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    type TEXT NOT NULL,
                    payload_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS sessions (
                    session_id TEXT PRIMARY KEY,
                    user_key TEXT NOT NULL,
                    port TEXT NOT NULL,
                    root_task_id TEXT NOT NULL,
                    state_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                );

                CREATE TABLE IF NOT EXISTS users (
                    user_key TEXT PRIMARY KEY,
                    profile_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                );

                CREATE TABLE IF NOT EXISTS session_messages (
                    message_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    message_index INTEGER NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    UNIQUE(session_id, message_index)
                );

                CREATE TABLE IF NOT EXISTS providers (
                    provider TEXT PRIMARY KEY,
                    api_key TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                );

                CREATE TABLE IF NOT EXISTS port_settings (
                    port TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    PRIMARY KEY (port, key)
                );

                CREATE TABLE IF NOT EXISTS port_bindings (
                    port TEXT NOT NULL,
                    conversation_key TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    agent_id TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    PRIMARY KEY (port, conversation_key)
                );

                CREATE TABLE IF NOT EXISTS port_session_ctx (
                    port TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    ctx_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    PRIMARY KEY (port, session_id)
                );

                CREATE TABLE IF NOT EXISTS agent_specs (
                    agent_id TEXT PRIMARY KEY,
                    model TEXT NOT NULL,
                    system_prompt TEXT NOT NULL,
                    tools_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                );

                CREATE TABLE IF NOT EXISTS policies (
                    policy_id TEXT PRIMARY KEY,
                    policy_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                );

                CREATE TABLE IF NOT EXISTS policies_use (
                    policy_id TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                    PRIMARY KEY(policy_id, entity_id)
                );

                UPDATE tasks
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE tasks
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE task_events
                SET ts = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(ts, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(ts)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(ts, 'unixepoch')) IS NULL
                   );
                UPDATE sessions
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE users
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE users
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE session_messages
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE providers
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE providers
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_settings
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_settings
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_bindings
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_bindings
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_session_ctx
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE port_session_ctx
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE agent_specs
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE agent_specs
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE policies
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );
                UPDATE policies
                SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(updated_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(updated_at, 'unixepoch')) IS NULL
                   );
                UPDATE policies_use
                SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE trim(coalesce(created_at, '')) = ''
                   OR (
                       strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at)) IS NULL
                       AND strftime('%Y-%m-%dT%H:%M:%SZ', datetime(created_at, 'unixepoch')) IS NULL
                   );

                INSERT OR IGNORE INTO providers(provider, api_key, created_at, updated_at)
                VALUES(
                    'kalosm',
                    '',
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                );
                "#,
            )
            .await
            .context("failed to run db migrations")?;

        info!(target: "borg_db", "control-plane/task migrations completed");
        Ok(())
    }
}
