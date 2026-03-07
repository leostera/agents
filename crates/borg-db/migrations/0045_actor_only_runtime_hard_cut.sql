PRAGMA foreign_keys = OFF;

DROP INDEX IF EXISTS idx_messages_session_created;
DROP INDEX IF EXISTS idx_port_bindings_port_session_id;
DROP INDEX IF EXISTS idx_schedule_jobs_target_actor_session;
DROP INDEX IF EXISTS idx_tool_calls_session_called_at;

DROP TABLE IF EXISTS messages;
CREATE TABLE messages (
    message_id TEXT PRIMARY KEY,
    sender_id TEXT,
    receiver_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL,
    reply_to_sender_id TEXT,
    reply_to_message_id TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_messages_receiver_status_created
ON messages(receiver_id, status, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_messages_receiver_created
ON messages(receiver_id, created_at ASC, message_id ASC);

DROP TABLE IF EXISTS port_bindings;
CREATE TABLE port_bindings (
    port TEXT NOT NULL,
    conversation_key TEXT NOT NULL,
    actor_id TEXT,
    context_snapshot_json TEXT,
    current_reasoning_effort TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (port, conversation_key)
);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_conversation
ON port_bindings(port, conversation_key);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_actor
ON port_bindings(port, actor_id);

CREATE INDEX IF NOT EXISTS idx_port_bindings_actor
ON port_bindings(actor_id);

DROP TABLE IF EXISTS schedule_job_runs;
DROP TABLE IF EXISTS schedule_jobs;

CREATE TABLE schedule_jobs (
    job_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    message_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    headers_json TEXT NOT NULL DEFAULT '{}',
    schedule_spec_json TEXT NOT NULL,
    next_run_at TEXT,
    last_run_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_schedule_jobs_status_next_run_at
ON schedule_jobs(status, next_run_at);

CREATE INDEX IF NOT EXISTS idx_schedule_jobs_target_actor
ON schedule_jobs(target_actor_id);

CREATE TABLE schedule_job_runs (
    run_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    scheduled_for TEXT NOT NULL,
    fired_at TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES schedule_jobs(job_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_schedule_job_runs_job_created
ON schedule_job_runs(job_id, created_at DESC);

DROP TABLE IF EXISTS tool_calls;
CREATE TABLE tool_calls (
    call_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT NOT NULL DEFAULT '{}',
    success INTEGER NOT NULL,
    error TEXT,
    duration_ms INTEGER,
    called_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_tool_calls_actor_called_at
ON tool_calls(actor_id, called_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_calls_called_at
ON tool_calls(called_at DESC);

PRAGMA foreign_keys = ON;
