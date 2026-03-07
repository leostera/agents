use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, ToolCallRecord};

impl BorgDb {
    pub async fn insert_tool_call(
        &self,
        call_id: &str,
        actor_id: &str,
        tool_name: &str,
        arguments_json: &Value,
        output_json: &Value,
        success: bool,
        error: Option<&str>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let call_id = call_id.to_string();
        let actor_id = actor_id.to_string();
        let tool_name = tool_name.to_string();
        let arguments_json = arguments_json.to_string();
        let output_json = output_json.to_string();
        let success_raw = if success { 1_i64 } else { 0_i64 };
        let error = error.map(ToOwned::to_owned);
        let duration_ms = duration_ms.and_then(|value| i64::try_from(value).ok());
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
                INSERT INTO tool_calls(
                    call_id,
                    actor_id,
                    tool_name,
                    arguments_json,
                    output_json,
                    success,
                    error,
                    duration_ms,
                    called_at
                )
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            call_id,
            actor_id,
            tool_name,
            arguments_json,
            output_json,
            success_raw,
            error,
            duration_ms,
            now
        )
        .execute(self.conn.pool())
        .await
        .context("failed to insert tool call")?;
        Ok(())
    }

    pub async fn list_tool_calls(&self, limit: usize) -> Result<Vec<ToolCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = match sqlx::query!(
            r#"
                SELECT
                    call_id as "call_id!: String",
                    actor_id as "actor_id!: String",
                    tool_name as "tool_name!: String",
                    arguments_json as "arguments_json!: String",
                    output_json as "output_json!: String",
                    success as "success!: i64",
                    error,
                    duration_ms,
                    called_at as "called_at!: String"
                FROM tool_calls
                ORDER BY called_at DESC
                LIMIT ?1
                "#,
            limit,
        )
        .fetch_all(self.conn.pool())
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

        rows.into_iter()
            .map(|row| {
                Ok(ToolCallRecord {
                    call_id: row.call_id,
                    actor_id: row.actor_id,
                    tool_name: row.tool_name,
                    arguments_json: serde_json::from_str(&row.arguments_json)
                        .unwrap_or_else(|_| Value::Object(Default::default())),
                    output_json: serde_json::from_str(&row.output_json)
                        .unwrap_or_else(|_| Value::Object(Default::default())),
                    success: row.success != 0,
                    error: row.error,
                    duration_ms: row.duration_ms.and_then(|value| u64::try_from(value).ok()),
                    called_at: parse_ts(&row.called_at)?,
                })
            })
            .collect()
    }

    pub async fn get_tool_call(&self, call_id: &str) -> Result<Option<ToolCallRecord>> {
        let call_id = call_id.to_string();
        let row = match sqlx::query!(
            r#"
                SELECT
                    call_id as "call_id!: String",
                    actor_id as "actor_id!: String",
                    tool_name as "tool_name!: String",
                    arguments_json as "arguments_json!: String",
                    output_json as "output_json!: String",
                    success as "success!: i64",
                    error,
                    duration_ms,
                    called_at as "called_at!: String"
                FROM tool_calls
                WHERE call_id = ?1
                LIMIT 1
                "#,
            call_id,
        )
        .fetch_optional(self.conn.pool())
        .await
        {
            Ok(row) => row,
            Err(err) => {
                if err.to_string().contains("no such table: tool_calls") {
                    return Ok(None);
                }
                return Err(err).context("failed to get tool call");
            }
        };

        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(ToolCallRecord {
            call_id: row.call_id,
            actor_id: row.actor_id,
            tool_name: row.tool_name,
            arguments_json: serde_json::from_str(&row.arguments_json)
                .unwrap_or_else(|_| Value::Object(Default::default())),
            output_json: serde_json::from_str(&row.output_json)
                .unwrap_or_else(|_| Value::Object(Default::default())),
            success: row.success != 0,
            error: row.error,
            duration_ms: row.duration_ms.and_then(|value| u64::try_from(value).ok()),
            called_at: parse_ts(&row.called_at)?,
        }))
    }
}
