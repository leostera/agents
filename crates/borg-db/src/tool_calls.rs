use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use sqlx::query;

use crate::utils::parse_ts;
use crate::{BorgDb, ToolCallRecord};

impl BorgDb {
    pub async fn insert_tool_call(
        &self,
        call_id: &str,
        session_id: &str,
        task_id: Option<&str>,
        tool_name: &str,
        arguments_json: &Value,
        output_json: &Value,
        success: bool,
        error: Option<&str>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        query(
            r#"
                INSERT INTO tool_calls(
                    call_id,
                    session_id,
                    task_id,
                    tool_name,
                    arguments_json,
                    output_json,
                    success,
                    error,
                    duration_ms,
                    called_at
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
        )
        .bind(call_id.to_string())
        .bind(session_id.to_string())
        .bind(task_id.map(ToOwned::to_owned))
        .bind(tool_name.to_string())
        .bind(arguments_json.to_string())
        .bind(output_json.to_string())
        .bind(if success { 1_i64 } else { 0_i64 })
        .bind(error.map(ToOwned::to_owned))
        .bind(duration_ms.and_then(|value| i64::try_from(value).ok()))
        .bind(now)
        .execute(self.conn.pool())
        .await
        .context("failed to insert tool call")?;
        Ok(())
    }

    pub async fn list_tool_calls(&self, limit: usize) -> Result<Vec<ToolCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let mut rows = match self
            .conn
            .query(
                r#"
                SELECT
                    call_id,
                    session_id,
                    task_id,
                    tool_name,
                    arguments_json,
                    output_json,
                    success,
                    error,
                    duration_ms,
                    called_at
                FROM tool_calls
                ORDER BY called_at DESC
                LIMIT ?1
                "#,
                (limit,),
            )
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                if err.to_string().contains("no such table: tool_calls") {
                    return Ok(Vec::new());
                }
                return Err(err).context("failed to list tool calls");
            }
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading tool call row")? {
            let args_raw: String = row.get(4)?;
            let output_raw: String = row.get(5)?;
            let success_raw: i64 = row.get(6)?;
            let duration_ms_raw: Option<i64> = row.get(8)?;
            let called_at: String = row.get(9)?;
            out.push(ToolCallRecord {
                call_id: row.get(0)?,
                session_id: row.get(1)?,
                task_id: row.get(2)?,
                tool_name: row.get(3)?,
                arguments_json: serde_json::from_str(&args_raw)
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                output_json: serde_json::from_str(&output_raw)
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                success: success_raw != 0,
                error: row.get(7)?,
                duration_ms: duration_ms_raw.and_then(|value| u64::try_from(value).ok()),
                called_at: parse_ts(&called_at)?,
            });
        }
        Ok(out)
    }

    pub async fn get_tool_call(&self, call_id: &str) -> Result<Option<ToolCallRecord>> {
        let mut rows = match self
            .conn
            .query(
                r#"
                SELECT
                    call_id,
                    session_id,
                    task_id,
                    tool_name,
                    arguments_json,
                    output_json,
                    success,
                    error,
                    duration_ms,
                    called_at
                FROM tool_calls
                WHERE call_id = ?1
                LIMIT 1
                "#,
                (call_id.to_string(),),
            )
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                if err.to_string().contains("no such table: tool_calls") {
                    return Ok(None);
                }
                return Err(err).context("failed to get tool call");
            }
        };

        let Some(row) = rows.next().await.context("failed reading tool call row")? else {
            return Ok(None);
        };
        let args_raw: String = row.get(4)?;
        let output_raw: String = row.get(5)?;
        let success_raw: i64 = row.get(6)?;
        let duration_ms_raw: Option<i64> = row.get(8)?;
        let called_at: String = row.get(9)?;
        Ok(Some(ToolCallRecord {
            call_id: row.get(0)?,
            session_id: row.get(1)?,
            task_id: row.get(2)?,
            tool_name: row.get(3)?,
            arguments_json: serde_json::from_str(&args_raw)
                .unwrap_or_else(|_| Value::Object(Default::default())),
            output_json: serde_json::from_str(&output_raw)
                .unwrap_or_else(|_| Value::Object(Default::default())),
            success: success_raw != 0,
            error: row.get(7)?,
            duration_ms: duration_ms_raw.and_then(|value| u64::try_from(value).ok()),
            called_at: parse_ts(&called_at)?,
        }))
    }
}
