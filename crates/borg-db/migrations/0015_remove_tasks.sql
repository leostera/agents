DROP TABLE IF EXISTS task_events;
DROP TABLE IF EXISTS deps;
DROP TABLE IF EXISTS tasks;

ALTER TABLE tool_calls RENAME TO tool_calls_old;

CREATE TABLE IF NOT EXISTS tool_calls (
    call_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT NOT NULL DEFAULT '{}',
    success INTEGER NOT NULL,
    error TEXT,
    duration_ms INTEGER,
    called_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO tool_calls(
    call_id,
    session_id,
    tool_name,
    arguments_json,
    output_json,
    success,
    error,
    duration_ms,
    called_at,
    created_at
)
SELECT
    call_id,
    session_id,
    tool_name,
    arguments_json,
    output_json,
    success,
    error,
    duration_ms,
    called_at,
    created_at
FROM tool_calls_old;

DROP TABLE tool_calls_old;

CREATE INDEX IF NOT EXISTS idx_tool_calls_session_called_at
ON tool_calls(session_id, called_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_calls_called_at
ON tool_calls(called_at DESC);
