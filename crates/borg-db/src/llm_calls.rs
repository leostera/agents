use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, LlmCallRecord};
use borg_core::{ActorId, LlmCallId, MessageId, ProviderId, WorkspaceId};

impl BorgDb {
    /// Persist the start of an LLM call.
    pub async fn insert_llm_call(
        &self,
        llm_call_id: &LlmCallId,
        workspace_id: &WorkspaceId,
        actor_id: &ActorId,
        message_id: &MessageId,
        provider_id: &ProviderId,
        model: &str,
        request_json: &Value,
    ) -> Result<()> {
        let llm_call_id = llm_call_id.to_string();
        let workspace_id = workspace_id.to_string();
        let actor_id = actor_id.to_string();
        let message_id = message_id.to_string();
        let provider_id = provider_id.to_string();
        let model = model.to_string();
        let request_json = serde_json::to_string(request_json)?;
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO llm_calls (
                llm_call_id, workspace_id, actor_id, message_id,
                provider_id, model, request_json, started_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            llm_call_id,
            workspace_id,
            actor_id,
            message_id,
            provider_id,
            model,
            request_json,
            now
        )
        .execute(self.pool())
        .await
        .context("failed to insert llm call")?;

        Ok(())
    }

    /// Persist the completion of an LLM call.
    pub async fn finish_llm_call(
        &self,
        llm_call_id: &LlmCallId,
        response_json: Option<&Value>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let llm_call_id = llm_call_id.to_string();
        let response_json = response_json
            .map(|v| serde_json::to_string(v))
            .transpose()?;
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            UPDATE llm_calls
            SET response_json = ?1,
                finished_at = ?2,
                error_code = ?3,
                error_message = ?4
            WHERE llm_call_id = ?5
            "#,
            response_json,
            now,
            error_code,
            error_message,
            llm_call_id
        )
        .execute(self.pool())
        .await
        .context("failed to finish llm call")?;

        Ok(())
    }

    pub async fn get_llm_call(&self, llm_call_id: &LlmCallId) -> Result<Option<LlmCallRecord>> {
        let id = llm_call_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                llm_call_id as "llm_call_id!: String",
                workspace_id as "workspace_id!: String",
                actor_id as "actor_id!: String",
                message_id as "message_id!: String",
                provider_id as "provider_id!: String",
                model as "model!: String",
                request_json as "request_json!: String",
                response_json,
                started_at as "started_at!: String",
                finished_at,
                error_code,
                error_message
            FROM llm_calls
            WHERE llm_call_id = ?1
            LIMIT 1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get llm call")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(LlmCallRecord {
            llm_call_id: LlmCallId::parse(&row.llm_call_id)?,
            workspace_id: WorkspaceId::parse(&row.workspace_id)?,
            actor_id: ActorId::parse(&row.actor_id)?,
            message_id: MessageId::parse(&row.message_id)?,
            provider_id: ProviderId::parse(&row.provider_id)?,
            model: row.model,
            request_json: serde_json::from_str(&row.request_json)?,
            response_json: row
                .response_json
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
            started_at: parse_ts(&row.started_at)?,
            finished_at: row.finished_at.map(|s| parse_ts(&s)).transpose()?,
            error_code: row.error_code,
            error_message: row.error_message,
        }))
    }

    pub async fn list_llm_calls(&self, limit: usize) -> Result<Vec<LlmCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(50);
        let rows = sqlx::query!(
            r#"
            SELECT
                llm_call_id as "llm_call_id!: String",
                workspace_id as "workspace_id!: String",
                actor_id as "actor_id!: String",
                message_id as "message_id!: String",
                provider_id as "provider_id!: String",
                model as "model!: String",
                request_json as "request_json!: String",
                response_json,
                started_at as "started_at!: String",
                finished_at,
                error_code,
                error_message
            FROM llm_calls
            ORDER BY started_at DESC
            LIMIT ?1
            "#,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list llm calls")?;

        rows.into_iter()
            .map(|row| {
                Ok(LlmCallRecord {
                    llm_call_id: LlmCallId::parse(&row.llm_call_id)?,
                    workspace_id: WorkspaceId::parse(&row.workspace_id)?,
                    actor_id: ActorId::parse(&row.actor_id)?,
                    message_id: MessageId::parse(&row.message_id)?,
                    provider_id: ProviderId::parse(&row.provider_id)?,
                    model: row.model,
                    request_json: serde_json::from_str(&row.request_json)?,
                    response_json: row
                        .response_json
                        .map(|s| serde_json::from_str(&s))
                        .transpose()?,
                    started_at: parse_ts(&row.started_at)?,
                    finished_at: row.finished_at.map(|s| parse_ts(&s)).transpose()?,
                    error_code: row.error_code,
                    error_message: row.error_message,
                })
            })
            .collect()
    }
}
