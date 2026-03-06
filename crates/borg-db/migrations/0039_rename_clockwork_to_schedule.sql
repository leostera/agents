ALTER TABLE clockwork_jobs RENAME TO schedule_jobs;
ALTER TABLE clockwork_job_runs RENAME TO schedule_job_runs;

DROP INDEX IF EXISTS idx_clockwork_jobs_status_next_run_at;
DROP INDEX IF EXISTS idx_clockwork_jobs_target_actor_session;
DROP INDEX IF EXISTS idx_clockwork_job_runs_job_created;

CREATE INDEX IF NOT EXISTS idx_schedule_jobs_status_next_run_at
ON schedule_jobs(status, next_run_at);

CREATE INDEX IF NOT EXISTS idx_schedule_jobs_target_actor_session
ON schedule_jobs(target_actor_id, target_session_id);

CREATE INDEX IF NOT EXISTS idx_schedule_job_runs_job_created
ON schedule_job_runs(job_id, created_at DESC);
