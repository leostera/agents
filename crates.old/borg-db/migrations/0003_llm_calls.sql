CREATE TABLE IF NOT EXISTS llm_calls (
    call_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    capability TEXT NOT NULL,
    model TEXT NOT NULL,
    success INTEGER NOT NULL,
    status_code INTEGER,
    status_reason TEXT,
    http_reason TEXT,
    error TEXT,
    latency_ms INTEGER,
    sent_at TEXT NOT NULL,
    received_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_llm_calls_sent_at
ON llm_calls(sent_at DESC);
