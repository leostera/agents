ALTER TABLE agent_specs RENAME TO agent_specs_old;

CREATE TABLE agent_specs (
    agent_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    default_provider_id TEXT,
    model TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO agent_specs(
    agent_id,
    name,
    enabled,
    default_provider_id,
    model,
    system_prompt,
    created_at,
    updated_at
)
SELECT
    agent_id,
    name,
    enabled,
    NULL,
    model,
    system_prompt,
    created_at,
    updated_at
FROM agent_specs_old;

DROP TABLE agent_specs_old;
