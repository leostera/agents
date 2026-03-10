DROP TABLE IF EXISTS behavior_capabilities;
DROP TABLE IF EXISTS behaviors;

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS actors_new (
    actor_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'RUNNING',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO actors_new (
    actor_id,
    name,
    system_prompt,
    status,
    created_at,
    updated_at
)
SELECT
    actor_id,
    name,
    system_prompt,
    status,
    created_at,
    updated_at
FROM actors;

DROP TABLE actors;
ALTER TABLE actors_new RENAME TO actors;

CREATE INDEX IF NOT EXISTS idx_actors_status_updated_at
ON actors(status, updated_at DESC);

PRAGMA foreign_keys = ON;
