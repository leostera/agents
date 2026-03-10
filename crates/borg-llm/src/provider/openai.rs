use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::error::{Error, LlmResult, OpenAIConfigError};
use crate::model::Model;
use crate::provider::LlmProvider;

#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub organization: Option<String>,
}

impl OpenAIConfig {
    pub fn new(api_key: impl Into<String>) -> Result<Self, OpenAIConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(OpenAIConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            base_url: "https://api.openai.com".to_string(),
            organization: None,
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }
}

pub struct OpenAI {
    client: Client,
    config: OpenAIConfig,
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
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsesRequest {
    pub model: String,
    pub input: Vec<ResponseInputItem>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseInputItem {
    Message { role: String, content: Vec<ResponseContent> },
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseContent {
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
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseOutputItem {
    Message { id: String, role: String, content: Vec<ResponseOutputContent> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseOutputContent {
    OutputText { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCreateRequest {
    pub model: String,
    pub dataset_id: String,
    pub subject: Option<String>,
    pub metrics: Option<Vec<EvalMetric>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalMetric {
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eval {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub status: String,
    pub model: String,
    pub dataset_id: String,
    pub metrics: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalListResponse {
    pub data: Vec<Eval>,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
    pub has_more: bool,
}

impl OpenAI {
    pub fn new(config: OpenAIConfig) -> Self {
        let client = Client::builder().build().expect("failed to build reqwest client");
        Self {
            client,
            config,
            cached_models: Arc::new(RwLock::new(None)),
        }
    }

    pub fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    pub async fn chat(&self, request: &ChatRequest) -> LlmResult<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        let auth = self.auth_header();

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json");

        if let Some(ref org) = self.config.organization {
            req_builder = req_builder.header("OpenAI-Organization", org);
        }

        let response = req_builder.json(request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openai".to_string(),
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
        let url = format!("{}/v1/responses", self.config.base_url);
        let auth = self.auth_header();

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openai".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: ResponsesResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn create_eval(&self, request: &EvalCreateRequest) -> LlmResult<Eval> {
        let url = format!("{}/v1/evals", self.config.base_url);
        let auth = self.auth_header();

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openai".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: Eval = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn list_evals(&self) -> LlmResult<EvalListResponse> {
        let url = format!("{}/v1/evals", self.config.base_url);
        let auth = self.auth_header();

        let response = self
            .client
            .get(&url)
            .header("Authorization", auth)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "openai".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: EvalListResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for OpenAI {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn capabilities(&self) -> &[Capability] {
        &[
            Capability::ChatCompletion,
            Capability::AudioTranscription,
            Capability::Evals,
        ]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        let mut cache = self.cached_models.write().await;
        if let Some(ref models) = *cache {
            return Ok(models.clone());
        }

        let models = vec![
            Model::new("gpt-4o"),
            Model::new("gpt-4o-mini"),
            Model::new("gpt-4-turbo"),
            Model::new("gpt-4"),
            Model::new("gpt-3.5-turbo"),
        ];

        *cache = Some(models.clone());
        Ok(models)
    }
}
