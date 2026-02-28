use anyhow::{Context, Result};
use serde_json::Value;

use crate::utils::parse_ts;
use crate::{BorgDb, LlmCallRecord};

impl BorgDb {
    pub async fn list_llm_calls(&self, limit: usize) -> Result<Vec<LlmCallRecord>> {
        let limit = i64::try_from(limit).unwrap_or(500);
        let mut rows = match self
            .conn
            .query(
                r#"
                SELECT
                    call_id,
                    provider,
                    capability,
                    model,
                    success,
                    status_code,
                    status_reason,
                    http_reason,
                    error,
                    latency_ms,
                    sent_at,
                    received_at
                FROM llm_calls
                ORDER BY sent_at DESC
                LIMIT ?1
                "#,
                (limit,),
            )
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

        let mut out = Vec::new();
        while let Some(row) = rows.next().await.context("failed reading llm call row")? {
            let success_raw: i64 = row.get(4)?;
            let status_code_raw: Option<i64> = row.get(5)?;
            let latency_ms_raw: Option<i64> = row.get(9)?;
            let sent_at: String = row.get(10)?;
            let received_at_raw: Option<String> = row.get(11)?;

            out.push(LlmCallRecord {
                call_id: row.get(0)?,
                provider: row.get(1)?,
                capability: row.get(2)?,
                model: row.get(3)?,
                success: success_raw != 0,
                status_code: status_code_raw.and_then(|value| u16::try_from(value).ok()),
                status_reason: row.get(6)?,
                http_reason: row.get(7)?,
                error: row.get(8)?,
                latency_ms: latency_ms_raw.and_then(|value| u64::try_from(value).ok()),
                sent_at: parse_ts(&sent_at)?,
                received_at: received_at_raw.as_deref().map(parse_ts).transpose()?,
                request_json: Value::Object(Default::default()),
                response_json: Value::Object(Default::default()),
                response_body: String::new(),
            });
        }

        Ok(out)
    }

    pub async fn get_llm_call(&self, call_id: &str) -> Result<Option<LlmCallRecord>> {
        let mut rows = match self
            .conn
            .query(
                r#"
                SELECT
                    call_id,
                    provider,
                    capability,
                    model,
                    success,
                    status_code,
                    status_reason,
                    http_reason,
                    error,
                    latency_ms,
                    sent_at,
                    received_at,
                    request_json,
                    response_json,
                    response_body
                FROM llm_calls
                WHERE call_id = ?1
                LIMIT 1
                "#,
                (call_id.to_string(),),
            )
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                if err.to_string().contains("no such column: request_json") {
                    let mut fallback = self
                        .conn
                        .query(
                            r#"
                            SELECT
                                call_id,
                                provider,
                                capability,
                                model,
                                success,
                                status_code,
                                status_reason,
                                http_reason,
                                error,
                                latency_ms,
                                sent_at,
                                received_at
                            FROM llm_calls
                            WHERE call_id = ?1
                            LIMIT 1
                            "#,
                            (call_id.to_string(),),
                        )
                        .await
                        .context("failed to query llm call fallback shape")?;
                    let Some(row) = fallback.next().await? else {
                        return Ok(None);
                    };
                    let success_raw: i64 = row.get(4)?;
                    let status_code_raw: Option<i64> = row.get(5)?;
                    let latency_ms_raw: Option<i64> = row.get(9)?;
                    let sent_at: String = row.get(10)?;
                    let received_at_raw: Option<String> = row.get(11)?;
                    return Ok(Some(LlmCallRecord {
                        call_id: row.get(0)?,
                        provider: row.get(1)?,
                        capability: row.get(2)?,
                        model: row.get(3)?,
                        success: success_raw != 0,
                        status_code: status_code_raw.and_then(|value| u16::try_from(value).ok()),
                        status_reason: row.get(6)?,
                        http_reason: row.get(7)?,
                        error: row.get(8)?,
                        latency_ms: latency_ms_raw.and_then(|value| u64::try_from(value).ok()),
                        sent_at: parse_ts(&sent_at)?,
                        received_at: received_at_raw.as_deref().map(parse_ts).transpose()?,
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

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let success_raw: i64 = row.get(4)?;
        let status_code_raw: Option<i64> = row.get(5)?;
        let latency_ms_raw: Option<i64> = row.get(9)?;
        let sent_at: String = row.get(10)?;
        let received_at_raw: Option<String> = row.get(11)?;
        let request_json_raw: Option<String> = row.get(12)?;
        let response_json_raw: Option<String> = row.get(13)?;
        let response_body: Option<String> = row.get(14)?;

        Ok(Some(LlmCallRecord {
            call_id: row.get(0)?,
            provider: row.get(1)?,
            capability: row.get(2)?,
            model: row.get(3)?,
            success: success_raw != 0,
            status_code: status_code_raw.and_then(|value| u16::try_from(value).ok()),
            status_reason: row.get(6)?,
            http_reason: row.get(7)?,
            error: row.get(8)?,
            latency_ms: latency_ms_raw.and_then(|value| u64::try_from(value).ok()),
            sent_at: parse_ts(&sent_at)?,
            received_at: received_at_raw.as_deref().map(parse_ts).transpose()?,
            request_json: parse_json_or_empty_object(request_json_raw),
            response_json: parse_json_or_empty_object(response_json_raw),
            response_body: response_body.unwrap_or_default(),
        }))
    }
}

fn parse_json_or_empty_object(raw: Option<String>) -> Value {
    raw.and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}
