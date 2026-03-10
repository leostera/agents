CREATE TABLE IF NOT EXISTS devmode_projects (
    project_id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS devmode_specs (
    spec_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'DRAFT',
    root_task_uri TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_devmode_specs_project_updated_at
ON devmode_specs(project_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_devmode_specs_status_updated_at
ON devmode_specs(status, updated_at DESC);
