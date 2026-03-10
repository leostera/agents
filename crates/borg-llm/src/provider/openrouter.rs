use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::error::{Error, LlmResult, OpenRouterConfigError};
use crate::model::Model;
use crate::provider::LlmProvider;

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub base_url: String,
}

impl OpenRouterConfig {
    pub fn new(api_key: impl Into<String>) -> Result<Self, OpenRouterConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(OpenRouterConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            base_url: "https://openrouter.ai".to_string(),
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

pub struct OpenRouter {
    client: Client,
    config: OpenRouterConfig,
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
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub seed: Option<i64>,
    pub logit_bias: Option<std::collections::HashMap<String, i32>>,
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
    pub system_fingerprint: Option<String>,
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
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsesRequest {
    pub model: String,
    pub input: Vec<InputItem>,
    pub text: Option<TextConfig>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InputItem {
    Message { role: String, content: Vec<InputContent> },
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InputContent {
    InputText { text: String },
    InputImage { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextConfig {
    pub format: Option<String>,
    pub verbosity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub output: Vec<OutputItem>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum OutputItem {
    Message { id: String, status: String, role: String, content: Vec<OutputContent> },
    Reasoning { id: String, reasoning: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum OutputContent {
    OutputText { text: String, annotations: Vec<serde_json::Value> },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsResponse {
    pub data: CreditsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsData {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub available_credits: f64,
    pub used_credits: f64,
    pub total_credits_limit: Option<f64>,
}

impl OpenRouter {
    pub fn new(config: OpenRouterConfig) -> Self {
        let client = Client::builder().build().expect("failed to build reqwest client");
        Self {
            client,
            config,
            cached_models: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn chat(&self, request: &ChatRequest) -> LlmResult<ChatResponse> {
        let url = format!("{}/api/v1/chat/completions", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/leostera/borg")
            .header("X-Title", "Borg")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openrouter".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn responses(&self, request: &ResponsesRequest) -> LlmResult<ResponsesResponse> {
        let url = format!("{}/api/v1/responses", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/leostera/borg")
            .header("X-Title", "Borg")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openrouter".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: ResponsesResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn credits(&self) -> LlmResult<CreditsResponse> {
        let url = format!("{}/api/v1/credits", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openrouter".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: CreditsResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for OpenRouter {
    fn provider_name(&self) -> &'static str {
        "openrouter"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::ChatCompletion, Capability::Completion]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        let mut cache = self.cached_models.write().await;
        if let Some(ref models) = *cache {
            return Ok(models.clone());
        }

        let models = vec![
            Model::new("openai/o4-mini"),
            Model::new("anthropic/claude-4.5-sonnet"),
            Model::new("google/gemini-2.5-pro"),
            Model::new("meta-llama/llama-3.3-70b-instruct"),
            Model::new("mistralai/mistral-large"),
        ];

        *cache = Some(models.clone());
        Ok(models)
    }
}
