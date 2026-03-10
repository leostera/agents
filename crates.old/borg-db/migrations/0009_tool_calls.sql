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

CREATE INDEX IF NOT EXISTS idx_tool_calls_session_called_at
ON tool_calls(session_id, called_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_calls_called_at
ON tool_calls(called_at DESC);
