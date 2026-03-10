use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{BorgDb, ScheduleJobRecord, ScheduleJobRunRecord};

#[derive(Debug, Clone)]
pub struct CreateScheduleJobInput {
    pub job_id: String,
    pub kind: String,
    pub target_actor_id: String,
    pub message_type: String,
    pub payload: Value,
    pub headers: Value,
    pub schedule_spec: Value,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateScheduleJobInput {
    pub kind: Option<String>,
    pub target_actor_id: Option<String>,
    pub message_type: Option<String>,
    pub payload: Option<Value>,
    pub headers: Option<Value>,
    pub schedule_spec: Option<Value>,
    pub next_run_at: Option<Option<String>>,
}

impl BorgDb {
    pub async fn list_schedule_jobs(
        &self,
        limit: usize,
        status: Option<&str>,
    ) -> Result<Vec<ScheduleJobRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = if let Some(status) = status {
            sqlx::query_as!(
                ScheduleJobRecord,
                r#"
                SELECT
                    job_id as "job_id!: String",
                    kind as "kind!: String",
                    status as "status!: String",
                    target_actor_id as "target_actor_id!: String",
                    message_type as "message_type!: String",
                    payload_json as "payload!: serde_json::Value",
                    headers_json as "headers!: serde_json::Value",
                    schedule_spec_json as "schedule_spec!: serde_json::Value",
                    next_run_at as "next_run_at: DateTime<Utc>",
                    last_run_at as "last_run_at: DateTime<Utc>",
                    created_at as "created_at!: DateTime<Utc>",
                    updated_at as "updated_at!: DateTime<Utc>"
                FROM schedule_jobs
                WHERE status = ?1
                ORDER BY updated_at DESC, job_id ASC
                LIMIT ?2
                "#,
                status,
                limit,
            )
            .fetch_all(self.conn.pool())
            .await
            .context("failed to list schedule jobs")?
        } else {
            sqlx::query_as!(
                ScheduleJobRecord,
                r#"
                SELECT
                    job_id as "job_id!: String",
                    kind as "kind!: String",
                    status as "status!: String",
                    target_actor_id as "target_actor_id!: String",
                    message_type as "message_type!: String",
                    payload_json as "payload!: serde_json::Value",
                    headers_json as "headers!: serde_json::Value",
                    schedule_spec_json as "schedule_spec!: serde_json::Value",
                    next_run_at as "next_run_at: DateTime<Utc>",
                    last_run_at as "last_run_at: DateTime<Utc>",
                    created_at as "created_at!: DateTime<Utc>",
                    updated_at as "updated_at!: DateTime<Utc>"
                FROM schedule_jobs
                ORDER BY updated_at DESC, job_id ASC
                LIMIT ?1
                "#,
                limit,
            )
            .fetch_all(self.conn.pool())
            .await
            .context("failed to list schedule jobs")?
        };
        Ok(rows)
    }

    pub async fn list_due_schedule_jobs(
        &self,
        now_rfc3339: &str,
        limit: usize,
    ) -> Result<Vec<ScheduleJobRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = sqlx::query_as!(
            ScheduleJobRecord,
            r#"
            SELECT
                job_id as "job_id!: String",
                kind as "kind!: String",
                status as "status!: String",
                target_actor_id as "target_actor_id!: String",
                message_type as "message_type!: String",
                payload_json as "payload!: serde_json::Value",
                headers_json as "headers!: serde_json::Value",
                schedule_spec_json as "schedule_spec!: serde_json::Value",
                next_run_at as "next_run_at: DateTime<Utc>",
                last_run_at as "last_run_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM schedule_jobs
            WHERE status = 'active'
              AND next_run_at IS NOT NULL
              AND next_run_at <= ?1
            ORDER BY next_run_at ASC, created_at ASC
            LIMIT ?2
            "#,
            now_rfc3339,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list due schedule jobs")?;
        Ok(rows)
    }

    pub async fn get_schedule_job(&self, job_id: &str) -> Result<Option<ScheduleJobRecord>> {
        let row = sqlx::query_as!(
            ScheduleJobRecord,
            r#"
            SELECT
                job_id as "job_id!: String",
                kind as "kind!: String",
                status as "status!: String",
                target_actor_id as "target_actor_id!: String",
                message_type as "message_type!: String",
                payload_json as "payload!: serde_json::Value",
                headers_json as "headers!: serde_json::Value",
                schedule_spec_json as "schedule_spec!: serde_json::Value",
                next_run_at as "next_run_at: DateTime<Utc>",
                last_run_at as "last_run_at: DateTime<Utc>",
                created_at as "created_at!: DateTime<Utc>",
                updated_at as "updated_at!: DateTime<Utc>"
            FROM schedule_jobs
            WHERE job_id = ?1
            LIMIT 1
            "#,
            job_id,
        )
        .fetch_optional(self.conn.pool())
        .await
        .context("failed to get schedule job")?;
        Ok(row)
    }

    pub async fn create_schedule_job(&self, input: &CreateScheduleJobInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let status = if input.kind == "once" && input.next_run_at.is_none() {
            "completed"
        } else {
            "active"
        };
        let payload_json = input.payload.to_string();
        let headers_json = input.headers.to_string();
        let schedule_spec_json = input.schedule_spec.to_string();
        sqlx::query!(
            r#"
            INSERT INTO schedule_jobs(
                job_id,
                kind,
                status,
                target_actor_id,
                message_type,
                payload_json,
                headers_json,
                schedule_spec_json,
                next_run_at,
                last_run_at,
                created_at,
                updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)
            "#,
            input.job_id,
            input.kind,
            status,
            input.target_actor_id,
            input.message_type,
            payload_json,
            headers_json,
            schedule_spec_json,
            input.next_run_at,
            now,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to create schedule job")?;
        Ok(())
    }

    pub async fn update_schedule_job(
        &self,
        job_id: &str,
        patch: &UpdateScheduleJobInput,
    ) -> Result<u64> {
        let current = self
            .get_schedule_job(job_id)
            .await?
            .ok_or_else(|| anyhow!("schedule job not found"))?;

        if current.status == "cancelled" || current.status == "completed" {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();
        let kind = patch.kind.clone().unwrap_or(current.kind);
        let target_actor_id = patch
            .target_actor_id
            .clone()
            .unwrap_or(current.target_actor_id);
        let message_type = patch.message_type.clone().unwrap_or(current.message_type);
        let payload = patch.payload.clone().unwrap_or(current.payload);
        let headers = patch.headers.clone().unwrap_or(current.headers);
        let schedule_spec = patch.schedule_spec.clone().unwrap_or(current.schedule_spec);
        let payload_json = payload.to_string();
        let headers_json = headers.to_string();
        let schedule_spec_json = schedule_spec.to_string();
        let next_run_at = patch
            .next_run_at
            .clone()
            .unwrap_or(current.next_run_at.map(|ts| ts.to_rfc3339()));

        let updated = sqlx::query!(
            r#"
            UPDATE schedule_jobs
            SET
                kind = ?2,
                target_actor_id = ?3,
                message_type = ?4,
                payload_json = ?5,
                headers_json = ?6,
                schedule_spec_json = ?7,
                next_run_at = ?8,
                updated_at = ?9
            WHERE job_id = ?1
            "#,
            job_id,
            kind,
            target_actor_id,
            message_type,
            payload_json,
            headers_json,
            schedule_spec_json,
            next_run_at,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to update schedule job")?
        .rows_affected();

        Ok(updated)
    }

    pub async fn set_schedule_job_status(&self, job_id: &str, status: &str) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query!(
            r#"
            UPDATE schedule_jobs
            SET status = ?2, updated_at = ?3
            WHERE job_id = ?1
            "#,
            job_id,
            status,
            now,
        )
        .execute(self.conn.pool())
        .await
        .context("failed to set schedule job status")?
        .rows_affected();
        Ok(updated)
    }

    pub async fn list_schedule_job_runs(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<ScheduleJobRunRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = sqlx::query_as!(
            ScheduleJobRunRecord,
            r#"
            SELECT
                run_id as "run_id!: String",
                job_id as "job_id!: String",
                scheduled_for as "scheduled_for!: DateTime<Utc>",
                fired_at as "fired_at!: DateTime<Utc>",
                target_actor_id as "target_actor_id!: String",
                message_id as "message_id!: String",
                created_at as "created_at!: DateTime<Utc>"
            FROM schedule_job_runs
            WHERE job_id = ?1
            ORDER BY created_at DESC, run_id DESC
            LIMIT ?2
            "#,
            job_id,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        .context("failed to list schedule job runs")?;
        Ok(rows)
    }
}
