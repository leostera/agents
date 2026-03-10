CREATE TABLE IF NOT EXISTS actors (
    actor_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'RUNNING',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_actors_status_updated_at
ON actors(status, updated_at DESC);

CREATE TABLE IF NOT EXISTS actor_mailbox (
    actor_message_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    session_id TEXT,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL,
    reply_to_actor_id TEXT,
    reply_to_message_id TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_actor_mailbox_actor_status_created
ON actor_mailbox(actor_id, status, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_actor_mailbox_status_created
ON actor_mailbox(status, created_at ASC);
