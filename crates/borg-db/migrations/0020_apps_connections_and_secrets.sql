ALTER TABLE apps ADD COLUMN source TEXT NOT NULL DEFAULT 'custom';
ALTER TABLE apps ADD COLUMN auth_strategy TEXT NOT NULL DEFAULT 'none';

UPDATE apps
SET source = 'managed'
WHERE built_in = 1;

CREATE TABLE IF NOT EXISTS app_connections (
    connection_id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL,
    owner_user_id TEXT,
    provider_account_id TEXT,
    external_user_id TEXT,
    status TEXT NOT NULL DEFAULT 'connected',
    connection_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(app_id) REFERENCES apps(app_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_connections_app_id
ON app_connections(app_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS app_secrets (
    secret_id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL,
    connection_id TEXT,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'opaque',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(app_id) REFERENCES apps(app_id) ON DELETE CASCADE,
    FOREIGN KEY(connection_id) REFERENCES app_connections(connection_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_secrets_app_id
ON app_secrets(app_id, updated_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_app_secrets_connection_key
ON app_secrets(app_id, COALESCE(connection_id, ''), key);
