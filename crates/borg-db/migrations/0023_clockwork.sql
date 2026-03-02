CREATE TABLE IF NOT EXISTS clockwork_jobs (
    job_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    target_session_id TEXT NOT NULL,
    message_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    headers_json TEXT NOT NULL DEFAULT '{}',
    schedule_spec_json TEXT NOT NULL,
    next_run_at TEXT,
    last_run_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_clockwork_jobs_status_next_run_at
ON clockwork_jobs(status, next_run_at);

CREATE INDEX IF NOT EXISTS idx_clockwork_jobs_target_actor_session
ON clockwork_jobs(target_actor_id, target_session_id);

CREATE TABLE IF NOT EXISTS clockwork_job_runs (
    run_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    scheduled_for TEXT NOT NULL,
    fired_at TEXT NOT NULL,
    target_actor_id TEXT NOT NULL,
    target_session_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_id) REFERENCES clockwork_jobs(job_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_clockwork_job_runs_job_created
ON clockwork_job_runs(job_id, created_at DESC);
