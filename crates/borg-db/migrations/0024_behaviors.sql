CREATE TABLE IF NOT EXISTS behaviors (
    behavior_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    preferred_provider_id TEXT,
    required_capabilities_json TEXT NOT NULL DEFAULT '[]',
    session_turn_concurrency TEXT NOT NULL DEFAULT 'serial',
    status TEXT NOT NULL DEFAULT 'ACTIVE',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_behaviors_status_updated_at
ON behaviors(status, updated_at DESC);

CREATE TABLE IF NOT EXISTS behavior_capabilities (
    behavior_id TEXT NOT NULL,
    capability_type TEXT NOT NULL,
    capability_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (behavior_id, capability_type, capability_id)
);

CREATE INDEX IF NOT EXISTS idx_behavior_capabilities_behavior_position
ON behavior_capabilities(behavior_id, position ASC);
