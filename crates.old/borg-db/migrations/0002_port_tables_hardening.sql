-- Port table hygiene and query performance indexes.

CREATE INDEX IF NOT EXISTS idx_port_settings_port_updated_at
ON port_settings(port, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_updated_at
ON port_bindings(port, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_session_id
ON port_bindings(port, session_id);

CREATE INDEX IF NOT EXISTS idx_port_session_ctx_port_updated_at
ON port_session_ctx(port, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_sessions_port_updated_at
ON sessions(port, updated_at DESC);

UPDATE port_settings
SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE created_at IS NULL OR trim(created_at) = '';

UPDATE port_settings
SET updated_at = created_at
WHERE updated_at IS NULL OR trim(updated_at) = '';

UPDATE port_bindings
SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE created_at IS NULL OR trim(created_at) = '';

UPDATE port_bindings
SET updated_at = created_at
WHERE updated_at IS NULL OR trim(updated_at) = '';

UPDATE port_session_ctx
SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE created_at IS NULL OR trim(created_at) = '';

UPDATE port_session_ctx
SET updated_at = created_at
WHERE updated_at IS NULL OR trim(updated_at) = '';

UPDATE sessions
SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE updated_at IS NULL OR trim(updated_at) = '';
