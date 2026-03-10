CREATE TABLE IF NOT EXISTS app_capabilities (
    capability_id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL,
    name TEXT NOT NULL,
    hint TEXT NOT NULL DEFAULT '',
    mode TEXT NOT NULL DEFAULT 'codemode',
    instructions TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(app_id) REFERENCES apps(app_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_capabilities_app_id
ON app_capabilities(app_id, updated_at DESC);
