CREATE TABLE IF NOT EXISTS taskgraph_tasks (
    uri TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    definition_of_done TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    assignee_agent_id TEXT NOT NULL,
    assignee_session_uri TEXT NOT NULL,
    reviewer_agent_id TEXT NOT NULL,
    reviewer_session_uri TEXT NOT NULL,
    parent_uri TEXT,
    duplicate_of TEXT,
    review_submitted_at TEXT,
    review_approved_at TEXT,
    review_changes_requested_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS taskgraph_task_labels (
    task_uri TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (task_uri, label),
    FOREIGN KEY (task_uri) REFERENCES taskgraph_tasks(uri) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS taskgraph_task_blocked_by (
    task_uri TEXT NOT NULL,
    blocked_by_uri TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (task_uri, blocked_by_uri),
    FOREIGN KEY (task_uri) REFERENCES taskgraph_tasks(uri) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS taskgraph_task_references (
    task_uri TEXT NOT NULL,
    reference_uri TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (task_uri, reference_uri),
    FOREIGN KEY (task_uri) REFERENCES taskgraph_tasks(uri) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS taskgraph_comments (
    id TEXT PRIMARY KEY,
    task_uri TEXT NOT NULL,
    author_session_uri TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (task_uri) REFERENCES taskgraph_tasks(uri) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS taskgraph_events (
    id TEXT PRIMARY KEY,
    task_uri TEXT NOT NULL,
    actor_session_uri TEXT NOT NULL,
    event_type TEXT NOT NULL,
    data_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (task_uri) REFERENCES taskgraph_tasks(uri) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_parent_uri ON taskgraph_tasks(parent_uri);
CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_assignee_session_status ON taskgraph_tasks(assignee_session_uri, status);
CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_reviewer_session_status ON taskgraph_tasks(reviewer_session_uri, status);
CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_status ON taskgraph_tasks(status);
CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_updated_at ON taskgraph_tasks(updated_at);
CREATE INDEX IF NOT EXISTS idx_taskgraph_tasks_duplicate_of ON taskgraph_tasks(duplicate_of);
CREATE INDEX IF NOT EXISTS idx_taskgraph_task_blocked_by_task_uri ON taskgraph_task_blocked_by(task_uri);
CREATE INDEX IF NOT EXISTS idx_taskgraph_task_blocked_by_blocked_by_uri ON taskgraph_task_blocked_by(blocked_by_uri);
CREATE INDEX IF NOT EXISTS idx_taskgraph_comments_task_created ON taskgraph_comments(task_uri, created_at, id);
CREATE INDEX IF NOT EXISTS idx_taskgraph_events_task_created ON taskgraph_events(task_uri, created_at, id);
