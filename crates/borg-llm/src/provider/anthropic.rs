use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::error::{Error, LlmResult, AnthropicConfigError};
use crate::model::Model;
use crate::provider::LlmProvider;

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub version: String,
    pub base_url: String,
}

impl AnthropicConfig {
    pub fn new(api_key: impl Into<String>) -> Result<Self, AnthropicConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AnthropicConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            version: "2023-06-01".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
        })
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

pub struct Anthropic {
    client: Client,
    config: AnthropicConfig,
    cached_models: Arc<RwLock<Option<Vec<Model>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Content,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    #[serde(rename_all = "camelCase")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub system: Option<Content>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub stop_sequences: Option<Vec<String>>,
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    pub content: Vec<ResponseContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ResponseContentBlock {
    Text { text: String },
    #[serde(rename_all = "camelCase")]
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Anthropic {
    pub fn new(config: AnthropicConfig) -> Self {
        let client = Client::builder().build().expect("failed to build reqwest client");
        Self {
            client,
            config,
            cached_models: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn chat(&self, request: &ChatRequest) -> LlmResult<ChatResponse> {
        let url = format!("{}/v1/messages", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", &self.config.version)
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "anthropic".to_string(),
                status: status.as_u16(),
                message: body,
            });
        }

        let body = response.text().await?;
        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for Anthropic {
    fn provider_name(&self) -> &'static str {
        "anthropic"
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::ChatCompletion]
    }

    async fn available_models(&self) -> LlmResult<Vec<Model>> {
        let mut cache = self.cached_models.write().await;
        if let Some(ref models) = *cache {
            return Ok(models.clone());
        }

        let models = vec![
            Model::new("claude-opus-4-5-20250520"),
            Model::new("claude-sonnet-4-5-20250520"),
            Model::new("claude-haiku-3-5-20250520"),
            Model::new("claude-3-opus-20240229"),
            Model::new("claude-3-sonnet-20240229"),
            Model::new("claude-3-haiku-20240307"),
        ];

        *cache = Some(models.clone());
        Ok(models)
    }
}
