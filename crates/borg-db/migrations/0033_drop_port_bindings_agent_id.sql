-- Remove legacy agent_id from port conversation bindings.
-- Runtime actor routing now uses actor_id, and session routing is independent.

CREATE TABLE IF NOT EXISTS port_bindings_v2 (
    port TEXT NOT NULL,
    conversation_key TEXT NOT NULL,
    session_id TEXT NOT NULL,
    actor_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (port, conversation_key)
);

INSERT INTO port_bindings_v2(
    port,
    conversation_key,
    session_id,
    actor_id,
    created_at,
    updated_at
)
SELECT
    port,
    conversation_key,
    session_id,
    actor_id,
    created_at,
    updated_at
FROM port_bindings;

DROP TABLE port_bindings;
ALTER TABLE port_bindings_v2 RENAME TO port_bindings;

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_updated_at
ON port_bindings(port, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_session_id
ON port_bindings(port, session_id);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_actor_id
ON port_bindings(port, actor_id);
