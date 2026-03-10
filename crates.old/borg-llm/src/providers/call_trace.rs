use std::time::Instant;

use borg_core::borgdir::BorgDir;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::Connection;
use tracing::{debug, error, info};
use uuid::Uuid;

pub struct ProviderCallTrace {
    call_id: String,
    provider: String,
    capability: String,
    model: String,
    request_json: String,
    sent_at: DateTime<Utc>,
    started: Instant,
}

impl ProviderCallTrace {
    pub fn sent(
        provider: impl Into<String>,
        capability: impl Into<String>,
        model: impl Into<String>,
        request_json: impl Serialize,
    ) -> Self {
        let sent_at = Utc::now();
        let call_id = format!("borg:llm_call:{}", Uuid::new_v4());
        let provider = provider.into();
        let capability = capability.into();
        let call = Self {
            call_id,
            provider,
            capability,
            model: model.into(),
            request_json: serde_json::to_string(&request_json).unwrap_or_else(|_| "{}".to_string()),
            sent_at,
            started: Instant::now(),
        };
        info!(
            target: "borg_llm",
            call_id = call.call_id.as_str(),
            provider = call.provider.as_str(),
            capability = call.capability.as_str(),
            model = call.model.as_str(),
            sent_at = %call.sent_at.to_rfc3339(),
            "provider call sent"
        );
        call
    }

    pub async fn succeeded(self, status: StatusCode, response_json: impl Serialize) {
        let received_at = Utc::now();
        let latency_ms = self.started.elapsed().as_millis() as u64;
        self.persist(
            true,
            Some(status),
            Some(status.canonical_reason().unwrap_or("unknown")),
            Some(status.canonical_reason().unwrap_or("unknown").to_string()),
            String::new(),
            String::new(),
            latency_ms,
            received_at,
            serde_json::to_string(&response_json).unwrap_or_else(|_| "{}".to_string()),
        )
        .await;
        info!(
            target: "borg_llm",
            call_id = self.call_id.as_str(),
            provider = self.provider.as_str(),
            capability = self.capability.as_str(),
            model = self.model.as_str(),
            sent_at = %self.sent_at.to_rfc3339(),
            received_at = %received_at.to_rfc3339(),
            latency_ms,
            status_code = status.as_u16(),
            status_reason = status.canonical_reason().unwrap_or("unknown"),
            "provider call succeeded"
        );
    }

    pub async fn failed(
        self,
        status: Option<StatusCode>,
        response_json_body: Option<&str>,
        response_body: Option<&str>,
        error_message: &str,
    ) {
        let received_at = Utc::now();
        let latency_ms = self.started.elapsed().as_millis() as u64;
        let status_reason = status.and_then(|value| value.canonical_reason());
        let response_body_opt = response_body;
        let normalized_response_json = response_json_body
            .or(response_body_opt)
            .unwrap_or("{}")
            .to_string();
        let response_body = response_body_opt.unwrap_or("").to_string();
        let error_message = error_message.to_string();
        let http_reason = response_json_body
            .and_then(extract_error_reason_from_body)
            .or_else(|| response_body_opt.and_then(extract_error_reason_from_body))
            .unwrap_or_else(|| status_reason.unwrap_or("unknown").to_string());
        self.persist(
            false,
            status,
            status_reason,
            Some(http_reason.clone()),
            error_message.clone(),
            response_body,
            latency_ms,
            received_at,
            normalized_response_json,
        )
        .await;
        error!(
            target: "borg_llm",
            call_id = self.call_id.as_str(),
            provider = self.provider.as_str(),
            capability = self.capability.as_str(),
            model = self.model.as_str(),
            sent_at = %self.sent_at.to_rfc3339(),
            received_at = %received_at.to_rfc3339(),
            latency_ms,
            status_code = status.map(|value| value.as_u16()),
            status_reason,
            http_reason,
            error = error_message,
            "provider call failed"
        );
    }

    async fn persist(
        &self,
        success: bool,
        status: Option<StatusCode>,
        status_reason: Option<&str>,
        http_reason: Option<String>,
        error_message: String,
        response_body: String,
        latency_ms: u64,
        received_at: DateTime<Utc>,
        response_json: String,
    ) {
        let config_path = BorgDir::new().config_db().to_string_lossy().to_string();
        let database_url = format!("sqlite://{config_path}");
        let mut conn = match sqlx::SqliteConnection::connect(&database_url).await {
            Ok(conn) => conn,
            Err(err) => {
                debug!(
                    target: "borg_llm",
                    error = %err,
                    path = config_path.as_str(),
                    "failed to connect to config db for llm call persistence"
                );
                return;
            }
        };

        let create_result = sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS llm_calls (
                call_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                capability TEXT NOT NULL,
                model TEXT NOT NULL,
                success INTEGER NOT NULL,
                status_code INTEGER,
                status_reason TEXT,
                http_reason TEXT,
                error TEXT,
                latency_ms INTEGER,
                sent_at TEXT NOT NULL,
                received_at TEXT,
                request_json TEXT NOT NULL DEFAULT '{}',
                response_json TEXT NOT NULL DEFAULT '{}',
                response_body TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            )
            "#,
        )
        .execute(&mut conn)
        .await;
        if let Err(err) = create_result {
            debug!(
                target: "borg_llm",
                error = %err,
                "failed to ensure llm_calls table exists"
            );
            return;
        }
        ensure_payload_columns(&mut conn).await;

        let insert_result = sqlx::query(
            r#"
            INSERT INTO llm_calls(
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
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
        )
        .bind(self.call_id.as_str())
        .bind(self.provider.as_str())
        .bind(self.capability.as_str())
        .bind(self.model.as_str())
        .bind(if success { 1_i64 } else { 0_i64 })
        .bind(status.map(|value| i64::from(value.as_u16())))
        .bind(status_reason)
        .bind(http_reason)
        .bind(error_message)
        .bind(i64::try_from(latency_ms).unwrap_or(i64::MAX))
        .bind(self.sent_at.to_rfc3339())
        .bind(received_at.to_rfc3339())
        .bind(self.request_json.clone())
        .bind(response_json)
        .bind(response_body)
        .execute(&mut conn)
        .await;
        if let Err(err) = insert_result {
            debug!(
                target: "borg_llm",
                error = %err,
                "failed to persist llm call"
            );
        }
    }
}

fn extract_error_reason_from_body(response_body: &str) -> Option<String> {
    let body = response_body.trim();
    if body.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<ErrorBody>(body) {
        return json.error_message();
    }

    Some(body.to_string())
}

#[derive(Debug, Deserialize)]
struct ErrorMessageObject {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: Option<ErrorField>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ErrorField {
    Text(String),
    Object(ErrorMessageObject),
}

impl ErrorBody {
    fn error_message(self) -> Option<String> {
        match self.error {
            Some(ErrorField::Text(text)) => normalize_message(text),
            Some(ErrorField::Object(obj)) => obj.message.and_then(normalize_message),
            None => self.message.and_then(normalize_message),
        }
    }
}

fn normalize_message(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn ensure_payload_columns(conn: &mut sqlx::SqliteConnection) {
    for statement in [
        "ALTER TABLE llm_calls ADD COLUMN request_json TEXT NOT NULL DEFAULT '{}'",
        "ALTER TABLE llm_calls ADD COLUMN response_json TEXT NOT NULL DEFAULT '{}'",
        "ALTER TABLE llm_calls ADD COLUMN response_body TEXT NOT NULL DEFAULT ''",
    ] {
        let result = sqlx::query(statement).execute(&mut *conn).await;
        if let Err(err) = result {
            let error_text = err.to_string();
            if error_text.contains("duplicate column name") {
                continue;
            }
            debug!(
                target: "borg_llm",
                error = %err,
                statement,
                "failed ensuring llm_calls payload column"
            );
        }
    }
}
