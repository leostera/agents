use anyhow::{Context, Result};
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, LlmCallRecord};

impl BorgDb {
    pub async fn list_llm_calls(&self, limit: usize) -> Result<Vec<LlmCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let rows = match sqlx::query!(
            r#"
                SELECT
                    call_id as "call_id!: String",
                    provider as "provider!: String",
                    capability as "capability!: String",
                    model as "model!: String",
                    success as "success!: i64",
                    status_code,
                    status_reason,
                    http_reason,
                    error,
                    latency_ms,
                    sent_at as "sent_at!: String",
                    received_at
                FROM llm_calls
                ORDER BY sent_at DESC
                LIMIT ?1
                "#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await
        {
            Ok(rows) => rows,
            Err(err) => {
                if err.to_string().contains("no such table: llm_calls") {
                    return Ok(Vec::new());
                }
                return Err(err).context("failed to list llm calls");
            }
        };

        rows.into_iter()
            .map(|row| {
                Ok(LlmCallRecord {
                    call_id: row.call_id,
                    provider: row.provider,
                    capability: row.capability,
                    model: row.model,
                    success: row.success != 0,
                    status_code: row.status_code.and_then(|value| u16::try_from(value).ok()),
                    status_reason: row.status_reason,
                    http_reason: row.http_reason,
                    error: row.error,
                    latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
                    sent_at: parse_ts(&row.sent_at)?,
                    received_at: row.received_at.as_deref().map(parse_ts).transpose()?,
                    request_json: Value::Object(Default::default()),
                    response_json: Value::Object(Default::default()),
                    response_body: String::new(),
                })
            })
            .collect()
    }

    pub async fn get_llm_call(&self, call_id: &str) -> Result<Option<LlmCallRecord>> {
        let call_id = call_id.to_string();
        let call_id_for_query = call_id.clone();
        let row = match sqlx::query!(
            r#"
                SELECT
                    call_id as "call_id!: String",
                    provider as "provider!: String",
                    capability as "capability!: String",
                    model as "model!: String",
                    success as "success!: i64",
                    status_code,
                    status_reason,
                    http_reason,
                    error,
                    latency_ms,
                    sent_at as "sent_at!: String",
                    received_at,
                    request_json,
                    response_json,
                    response_body
                FROM llm_calls
                WHERE call_id = ?1
                LIMIT 1
                "#,
            call_id_for_query,
        )
        .fetch_optional(self.conn.pool())
        .await
        {
            Ok(row) => row,
            Err(err) => {
                if err.to_string().contains("no such column: request_json") {
                    let row = sqlx::query!(
                        r#"
                            SELECT
                                call_id as "call_id!: String",
                                provider as "provider!: String",
                                capability as "capability!: String",
                                model as "model!: String",
                                success as "success!: i64",
                                status_code,
                                status_reason,
                                http_reason,
                                error,
                                latency_ms,
                                sent_at as "sent_at!: String",
                                received_at
                            FROM llm_calls
                            WHERE call_id = ?1
                            LIMIT 1
                            "#,
                        call_id,
                    )
                    .fetch_optional(self.conn.pool())
                    .await
                    .context("failed to query llm call fallback shape")?;
                    let Some(row) = row else {
                        return Ok(None);
                    };
                    return Ok(Some(LlmCallRecord {
                        call_id: row.call_id,
                        provider: row.provider,
                        capability: row.capability,
                        model: row.model,
                        success: row.success != 0,
                        status_code: row.status_code.and_then(|value| u16::try_from(value).ok()),
                        status_reason: row.status_reason,
                        http_reason: row.http_reason,
                        error: row.error,
                        latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
                        sent_at: parse_ts(&row.sent_at)?,
                        received_at: row.received_at.as_deref().map(parse_ts).transpose()?,
                        request_json: Value::Object(Default::default()),
                        response_json: Value::Object(Default::default()),
                        response_body: String::new(),
                    }));
                }
                if err.to_string().contains("no such table: llm_calls") {
                    return Ok(None);
                }
                return Err(err).context("failed to get llm call");
            }
        };

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(LlmCallRecord {
            call_id: row.call_id,
            provider: row.provider,
            capability: row.capability,
            model: row.model,
            success: row.success != 0,
            status_code: row.status_code.and_then(|value| u16::try_from(value).ok()),
            status_reason: row.status_reason,
            http_reason: row.http_reason,
            error: row.error,
            latency_ms: row.latency_ms.and_then(|value| u64::try_from(value).ok()),
            sent_at: parse_ts(&row.sent_at)?,
            received_at: row.received_at.as_deref().map(parse_ts).transpose()?,
            request_json: parse_json_or_empty_object(Some(row.request_json)),
            response_json: parse_json_or_empty_object(Some(row.response_json)),
            response_body: row.response_body,
        }))
    }
}

fn parse_json_or_empty_object(raw: Option<String>) -> Value {
    raw.and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}
