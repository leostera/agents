use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

use crate::Provider;

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Clone)]
pub struct OpenAiProvider {
    http: Client,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(&self, model: &str, messages: &[Value], tools: &[Value]) -> Result<Value> {
        let body = json!({
            "model": model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
        });

        let response = self
            .http
            .post(OPENAI_CHAT_COMPLETIONS_URL)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!("openai chat completions returned {}", response.status()));
        }

        let payload: Value = response.json().await?;
        payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
            .ok_or_else(|| anyhow!("missing choices[0].message"))
    }
}
