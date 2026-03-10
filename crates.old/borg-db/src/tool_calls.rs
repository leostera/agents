use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, ToolCallRecord};
use borg_core::{ActorId, MessageId, ToolCallId, ToolCallStatus, WorkspaceId};

impl BorgDb {
    /// Persist the start of a tool call.
    pub async fn insert_tool_call(
        &self,
        tool_call_id: &ToolCallId,
        workspace_id: &WorkspaceId,
        actor_id: &ActorId,
        message_id: &MessageId,
        tool_name: &str,
        request_json: &Value,
    ) -> Result<()> {
        let tool_call_id = tool_call_id.to_string();
        let workspace_id = workspace_id.to_string();
        let actor_id = actor_id.to_string();
        let message_id = message_id.to_string();
        let tool_name = tool_name.to_string();
        let request_json = serde_json::to_string(request_json)?;
        let status = ToolCallStatus::Running.to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO tool_calls (
                tool_call_id, workspace_id, actor_id, message_id,
                tool_name, request_json, status, started_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            tool_call_id,
            workspace_id,
            actor_id,
            message_id,
            tool_name,
            request_json,
            status,
            now
        )
        .execute(self.pool())
        .await
        .context("failed to insert tool call")?;

        Ok(())
    }

    /// Persist the completion of a tool call.
    pub async fn finish_tool_call(
        &self,
        tool_call_id: &ToolCallId,
        result_json: Option<&Value>,
        status: ToolCallStatus,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let tool_call_id = tool_call_id.to_string();
        let result_json = result_json.map(|v| serde_json::to_string(v)).transpose()?;
        let status = status.to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            UPDATE tool_calls
            SET result_json = ?1,
                status = ?2,
                finished_at = ?3,
                error_code = ?4,
                error_message = ?5
            WHERE tool_call_id = ?6
            "#,
            result_json,
            status,
            now,
            error_code,
            error_message,
            tool_call_id
        )
        .execute(self.pool())
        .await
        .context("failed to finish tool call")?;

        Ok(())
    }

    pub async fn get_tool_call(&self, tool_call_id: &ToolCallId) -> Result<Option<ToolCallRecord>> {
        let id = tool_call_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                tool_call_id as "tool_call_id!: String",
                workspace_id as "workspace_id!: String",
                actor_id as "actor_id!: String",
                message_id as "message_id!: String",
                tool_name as "tool_name!: String",
                request_json as "request_json!: String",
                result_json,
                status as "status!: String",
                started_at as "started_at!: String",
                finished_at,
                error_code,
                error_message
            FROM tool_calls
            WHERE tool_call_id = ?1
            LIMIT 1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get tool call")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(ToolCallRecord {
            tool_call_id: ToolCallId::from_id(&row.tool_call_id),
            workspace_id: WorkspaceId::from_id(&row.workspace_id),
            actor_id: ActorId::parse(&row.actor_id)?,
            message_id: MessageId::from_id(&row.message_id),
            tool_name: row.tool_name,
            request_json: serde_json::from_str(&row.request_json)?,
            result_json: row
                .result_json
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
            status: ToolCallStatus::parse(&row.status)?,
            started_at: parse_ts(&row.started_at)?,
            finished_at: row.finished_at.map(|s| parse_ts(&s)).transpose()?,
            error_code: row.error_code,
            error_message: row.error_message,
        }))
    }

    pub async fn list_tool_calls(&self, limit: usize) -> Result<Vec<ToolCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(50);
        let rows = sqlx::query!(
            r#"
            SELECT
                tool_call_id as "tool_call_id!: String",
                workspace_id as "workspace_id!: String",
                actor_id as "actor_id!: String",
                message_id as "message_id!: String",
                tool_name as "tool_name!: String",
                request_json as "request_json!: String",
                result_json,
                status as "status!: String",
                started_at as "started_at!: String",
                finished_at,
                error_code,
                error_message
            FROM tool_calls
            ORDER BY started_at DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list tool calls")?;

        rows.into_iter()
            .map(|row| {
                Ok(ToolCallRecord {
                    tool_call_id: ToolCallId::from_id(&row.tool_call_id),
                    workspace_id: WorkspaceId::from_id(&row.workspace_id),
                    actor_id: ActorId::parse(&row.actor_id)?,
                    message_id: MessageId::from_id(&row.message_id),
                    tool_name: row.tool_name,
                    request_json: serde_json::from_str(&row.request_json)?,
                    result_json: row
                        .result_json
                        .map(|s| serde_json::from_str(&s))
                        .transpose()?,
                    status: ToolCallStatus::parse(&row.status)?,
                    started_at: parse_ts(&row.started_at)?,
                    finished_at: row.finished_at.map(|s| parse_ts(&s)).transpose()?,
                    error_code: row.error_code,
                    error_message: row.error_message,
                })
            })
            .collect()
    }
}
