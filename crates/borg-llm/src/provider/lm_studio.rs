use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::error::{Error, LlmResult};
use crate::model::Model;
use crate::provider::LlmProvider;

#[derive(Debug, Clone)]
pub struct LmStudioConfig {
    pub base_url: String,
    pub api_token: Option<String>,
}

impl LmStudioConfig {
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:1234".to_string(),
            api_token: None,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        if !base_url.is_empty() {
            self.base_url = base_url;
        }
        self
    }

    pub fn with_api_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }
}

impl Default for LmStudioConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LmStudio {
    client: Client,
    config: LmStudioConfig,
    cached_models: Arc<RwLock<Option<Vec<Model>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub repeat_penalty: Option<f32>,
    pub seed: Option<i64>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub r#type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolChoice {
    pub r#type: String,
    pub function: Option<ToolChoiceFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolChoiceFunction {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseFormat {
    pub r#type: String,
    pub json_schema: Option<JsonSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonSchema {
    pub name: String,
    pub strict: Option<bool>,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
}

impl LmStudio {
    pub fn new(config: LmStudioConfig) -> Self {
        let client = Client::builder().build().expect("failed to build reqwest client");
        Self {
            client,
            config,
            cached_models: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn chat(&self, request: &ChatRequest) -> LlmResult<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(ref token) = self.config.api_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = req_builder.json(request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "lm_studio".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn list_models(&self) -> LlmResult<Vec<Model>> {
        let url = format!("{}/v1/models", self.config.base_url);

        let mut req_builder = self.client.get(&url);

        if let Some(ref token) = self.config.api_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = req_builder.send().await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body = response.text().await?;
        let parsed: ModelsResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;

        Ok(parsed.data.into_iter().map(|m| Model::new(m.id)).collect())
    }
}

#[async_trait]
impl LlmProvider for LmStudio {
    fn provider_name(&self) -> &'static str {
        "lm_studio"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::ChatCompletion]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        let mut cache = self.cached_models.write().await;
        if let Some(ref models) = *cache {
            return Ok(models.clone());
        }

        let models = self.list_models().await?;
        *cache = Some(models.clone());
        Ok(models)
    }
}
