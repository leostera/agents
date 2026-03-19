use reqwest::Client;
use serde_json::Value;

use super::{RunResponseEnvelope, RunResult};
use crate::llm::error::{Error, LlmResult};

pub(super) async fn execute_run_request(
    client: &Client,
    model: &str,
    body: Value,
    api_token: &str,
    base_url: &str,
) -> LlmResult<RunResult> {
    let url = format!("{base_url}/run/{model}");
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(Error::Provider {
            provider: "workers_ai".to_string(),
            status: status.as_u16(),
            message: body,
        });
    }

    let envelope: RunResponseEnvelope =
        serde_json::from_str(&body).map_err(|error| Error::parse(&body, error))?;
    if !envelope.success {
        return Err(Error::Provider {
            provider: "workers_ai".to_string(),
            status: status.as_u16(),
            message: body,
        });
    }

    Ok(envelope.result)
}
