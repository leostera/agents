CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    sender_id TEXT,
    receiver_id TEXT NOT NULL,
    session_id TEXT,
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

CREATE INDEX IF NOT EXISTS idx_messages_session_created
ON messages(session_id, created_at ASC, message_id ASC);

DROP TABLE IF EXISTS actor_mailbox;
DROP TABLE IF EXISTS session_messages;
DROP TABLE IF EXISTS session_message_cursors;
