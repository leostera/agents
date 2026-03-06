-- Workspace model simplification hard-cut:
-- - context snapshots move to port_bindings
-- - legacy sessions/session_messages/session_context tables are removed

ALTER TABLE port_bindings
ADD COLUMN context_snapshot_json TEXT;

ALTER TABLE port_bindings
ADD COLUMN current_reasoning_effort TEXT;

DROP INDEX IF EXISTS idx_sessions_port_updated_at;
DROP INDEX IF EXISTS idx_port_session_ctx_port_updated_at;

DROP TABLE IF EXISTS session_message_cursors;
DROP TABLE IF EXISTS session_messages;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS port_session_ctx;
