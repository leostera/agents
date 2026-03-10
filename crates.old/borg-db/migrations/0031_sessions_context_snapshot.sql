ALTER TABLE sessions
ADD COLUMN context_snapshot_json TEXT;

UPDATE sessions
SET context_snapshot_json = (
    SELECT p.ctx_json
    FROM port_session_ctx p
    WHERE p.session_id = sessions.session_id
    ORDER BY p.updated_at DESC
    LIMIT 1
)
WHERE context_snapshot_json IS NULL;
