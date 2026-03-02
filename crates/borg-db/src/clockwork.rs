use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{BorgDb, ClockworkJobRecord, ClockworkJobRunRecord};

#[derive(Debug, Clone)]
pub struct CreateClockworkJobInput {
    pub job_id: String,
    pub kind: String,
    pub target_actor_id: String,
    pub target_session_id: String,
    pub message_type: String,
    pub payload: Value,
    pub headers: Value,
    pub schedule_spec: Value,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateClockworkJobInput {
    pub kind: Option<String>,
    pub target_actor_id: Option<String>,
    pub target_session_id: Option<String>,
    pub message_type: Option<String>,
    pub payload: Option<Value>,
    pub headers: Option<Value>,
    pub schedule_spec: Option<Value>,
    pub next_run_at: Option<Option<String>>,
}

impl BorgDb {
    pub async fn list_clockwork_jobs(
        &self,
        limit: usize,
        status: Option<&str>,
    ) -> Result<Vec<ClockworkJobRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = if let Some(status) = status {
            sqlx::query(
                r#"
                SELECT
                    job_id,
                    kind,
                    status,
                    target_actor_id,
                    target_session_id,
                    message_type,
                    payload_json,
                    headers_json,
                    schedule_spec_json,
                    next_run_at,
                    last_run_at,
                    created_at,
                    updated_at
                FROM clockwork_jobs
                WHERE status = ?1
                ORDER BY updated_at DESC, job_id ASC
                LIMIT ?2
                "#,
            )
            .bind(status)
            .bind(limit)
            .fetch_all(self.conn.pool())
            .await
            .context("failed to list clockwork jobs")?
        } else {
            sqlx::query(
                r#"
                SELECT
                    job_id,
                    kind,
                    status,
                    target_actor_id,
                    target_session_id,
                    message_type,
                    payload_json,
                    headers_json,
                    schedule_spec_json,
                    next_run_at,
                    last_run_at,
                    created_at,
                    updated_at
                FROM clockwork_jobs
                ORDER BY updated_at DESC, job_id ASC
                LIMIT ?1
                "#,
            )
            .bind(limit)
            .fetch_all(self.conn.pool())
            .await
            .context("failed to list clockwork jobs")?
        };

        rows.into_iter().map(clockwork_job_from_row).collect()
    }

    pub async fn list_due_clockwork_jobs(
        &self,
        now_rfc3339: &str,
        limit: usize,
    ) -> Result<Vec<ClockworkJobRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = sqlx::query(
            r#"
            SELECT
                job_id,
                kind,
                status,
                target_actor_id,
                target_session_id,
                message_type,
                payload_json,
                headers_json,
                schedule_spec_json,
                next_run_at,
                last_run_at,
                created_at,
                updated_at
            FROM clockwork_jobs
            WHERE status = 'active'
              AND next_run_at IS NOT NULL
              AND next_run_at <= ?1
            ORDER BY next_run_at ASC, created_at ASC
            LIMIT ?2
            "#,
        )
        .bind(now_rfc3339)
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list due clockwork jobs")?;

        rows.into_iter().map(clockwork_job_from_row).collect()
    }

    pub async fn get_clockwork_job(&self, job_id: &str) -> Result<Option<ClockworkJobRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                job_id,
                kind,
                status,
                target_actor_id,
                target_session_id,
                message_type,
                payload_json,
                headers_json,
                schedule_spec_json,
                next_run_at,
                last_run_at,
                created_at,
                updated_at
            FROM clockwork_jobs
            WHERE job_id = ?1
            LIMIT 1
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get clockwork job")?;

        row.map(clockwork_job_from_row).transpose()
    }

    pub async fn create_clockwork_job(&self, input: &CreateClockworkJobInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let status = if input.kind == "once" && input.next_run_at.is_none() {
            "completed"
        } else {
            "active"
        };
        sqlx::query(
            r#"
            INSERT INTO clockwork_jobs(
                job_id,
                kind,
                status,
                target_actor_id,
                target_session_id,
                message_type,
                payload_json,
                headers_json,
                schedule_spec_json,
                next_run_at,
                last_run_at,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, ?12)
            "#,
        )
        .bind(&input.job_id)
        .bind(&input.kind)
        .bind(status)
        .bind(&input.target_actor_id)
        .bind(&input.target_session_id)
        .bind(&input.message_type)
        .bind(input.payload.to_string())
        .bind(input.headers.to_string())
        .bind(input.schedule_spec.to_string())
        .bind(input.next_run_at.clone())
        .bind(now.clone())
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to create clockwork job")?;
        Ok(())
    }

    pub async fn update_clockwork_job(
        &self,
        job_id: &str,
        patch: &UpdateClockworkJobInput,
    ) -> Result<u64> {
        let current = self
            .get_clockwork_job(job_id)
            .await?
            .ok_or_else(|| anyhow!("clockwork job not found"))?;

        if current.status == "cancelled" || current.status == "completed" {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();
        let kind = patch.kind.clone().unwrap_or(current.kind);
        let target_actor_id = patch
            .target_actor_id
            .clone()
            .unwrap_or(current.target_actor_id);
        let target_session_id = patch
            .target_session_id
            .clone()
            .unwrap_or(current.target_session_id);
        let message_type = patch.message_type.clone().unwrap_or(current.message_type);
        let payload = patch.payload.clone().unwrap_or(current.payload);
        let headers = patch.headers.clone().unwrap_or(current.headers);
        let schedule_spec = patch.schedule_spec.clone().unwrap_or(current.schedule_spec);
        let next_run_at = patch
            .next_run_at
            .clone()
            .unwrap_or(current.next_run_at.map(|ts| ts.to_rfc3339()));

        let updated = sqlx::query(
            r#"
            UPDATE clockwork_jobs
            SET
                kind = ?2,
                target_actor_id = ?3,
                target_session_id = ?4,
                message_type = ?5,
                payload_json = ?6,
                headers_json = ?7,
                schedule_spec_json = ?8,
                next_run_at = ?9,
                updated_at = ?10
            WHERE job_id = ?1
            "#,
        )
        .bind(job_id)
        .bind(kind)
        .bind(target_actor_id)
        .bind(target_session_id)
        .bind(message_type)
        .bind(payload.to_string())
        .bind(headers.to_string())
        .bind(schedule_spec.to_string())
        .bind(next_run_at)
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to update clockwork job")?
        .rows_affected();

        Ok(updated)
    }

    pub async fn set_clockwork_job_status(&self, job_id: &str, status: &str) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query(
            r#"
            UPDATE clockwork_jobs
            SET status = ?2, updated_at = ?3
            WHERE job_id = ?1
            "#,
        )
        .bind(job_id)
        .bind(status)
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to set clockwork job status")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn list_clockwork_job_runs(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<ClockworkJobRunRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = sqlx::query(
            r#"
            SELECT
                run_id,
                job_id,
                scheduled_for,
                fired_at,
                target_actor_id,
                target_session_id,
                message_id,
                created_at
            FROM clockwork_job_runs
            WHERE job_id = ?1
            ORDER BY created_at DESC, run_id DESC
            LIMIT ?2
            "#,
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list clockwork job runs")?;

        rows.into_iter().map(clockwork_job_run_from_row).collect()
    }
}

fn clockwork_job_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ClockworkJobRecord> {
    let payload_json = row.try_get::<String, _>("payload_json")?;
    let headers_json = row.try_get::<String, _>("headers_json")?;
    let schedule_spec_json = row.try_get::<String, _>("schedule_spec_json")?;
    let next_run_at = row.try_get::<Option<String>, _>("next_run_at")?;
    let last_run_at = row.try_get::<Option<String>, _>("last_run_at")?;

    Ok(ClockworkJobRecord {
        job_id: row.try_get("job_id")?,
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        target_actor_id: row.try_get("target_actor_id")?,
        target_session_id: row.try_get("target_session_id")?,
        message_type: row.try_get("message_type")?,
        payload: serde_json::from_str(&payload_json).unwrap_or(Value::Null),
        headers: serde_json::from_str(&headers_json).unwrap_or(Value::Object(Default::default())),
        schedule_spec: serde_json::from_str(&schedule_spec_json)
            .unwrap_or(Value::Object(Default::default())),
        next_run_at: next_run_at.as_deref().map(parse_ts).transpose()?,
        last_run_at: last_run_at.as_deref().map(parse_ts).transpose()?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

fn clockwork_job_run_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ClockworkJobRunRecord> {
    Ok(ClockworkJobRunRecord {
        run_id: row.try_get("run_id")?,
        job_id: row.try_get("job_id")?,
        scheduled_for: parse_ts(&row.try_get::<String, _>("scheduled_for")?)?,
        fired_at: parse_ts(&row.try_get::<String, _>("fired_at")?)?,
        target_actor_id: row.try_get("target_actor_id")?,
        target_session_id: row.try_get("target_session_id")?,
        message_id: row.try_get("message_id")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
    })
}
