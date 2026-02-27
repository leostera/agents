use std::time::Instant;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde_json::Value;
use tracing::{error, info};

pub struct ProviderCallTrace {
    provider: &'static str,
    capability: &'static str,
    model: String,
    request_json: Value,
    sent_at: DateTime<Utc>,
    started: Instant,
}

impl ProviderCallTrace {
    pub fn sent(
        provider: &'static str,
        capability: &'static str,
        model: impl Into<String>,
        request_json: Value,
    ) -> Self {
        let sent_at = Utc::now();
        let call = Self {
            provider,
            capability,
            model: model.into(),
            request_json,
            sent_at,
            started: Instant::now(),
        };
        info!(
            target: "borg_llm",
            provider = call.provider,
            capability = call.capability,
            model = call.model.as_str(),
            sent_at = %call.sent_at.to_rfc3339(),
            request_json = %call.request_json,
            "provider call sent"
        );
        call
    }

    pub fn succeeded(self, status: StatusCode, response_json: &Value) {
        let received_at = Utc::now();
        let latency_ms = self.started.elapsed().as_millis() as u64;
        info!(
            target: "borg_llm",
            provider = self.provider,
            capability = self.capability,
            model = self.model.as_str(),
            sent_at = %self.sent_at.to_rfc3339(),
            received_at = %received_at.to_rfc3339(),
            latency_ms,
            status_code = status.as_u16(),
            request_json = %self.request_json,
            response_json = %response_json,
            "provider call succeeded"
        );
    }

    pub fn failed(
        self,
        status: Option<StatusCode>,
        response_json: Option<&Value>,
        response_body: Option<&str>,
        error_message: &str,
    ) {
        let received_at = Utc::now();
        let latency_ms = self.started.elapsed().as_millis() as u64;
        error!(
            target: "borg_llm",
            provider = self.provider,
            capability = self.capability,
            model = self.model.as_str(),
            sent_at = %self.sent_at.to_rfc3339(),
            received_at = %received_at.to_rfc3339(),
            latency_ms,
            status_code = status.map(|value| value.as_u16()),
            error = error_message,
            request_json = %self.request_json,
            response_json = response_json.map(|value| value.to_string()),
            response_body,
            "provider call failed"
        );
    }
}
