CREATE TABLE IF NOT EXISTS session_message_cursors (
    session_id TEXT PRIMARY KEY,
    last_index INTEGER NOT NULL DEFAULT -1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
);

INSERT INTO session_message_cursors(session_id, last_index, updated_at)
SELECT
    sm.session_id,
    MAX(sm.message_index) AS last_index,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now') AS updated_at
FROM session_messages sm
JOIN sessions s
  ON s.session_id = sm.session_id
GROUP BY sm.session_id
ON CONFLICT(session_id) DO UPDATE SET
    last_index = excluded.last_index,
    updated_at = excluded.updated_at;
