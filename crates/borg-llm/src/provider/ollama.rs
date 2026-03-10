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
pub struct OllamaConfig {
    pub base_url: String,
}

impl OllamaConfig {
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        if !base_url.is_empty() {
            self.base_url = base_url;
        }
        self
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Ollama {
    client: Client,
    config: OllamaConfig,
    cached_models: Arc<RwLock<Option<Vec<Model>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub images: Option<Vec<String>>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: Option<bool>,
    pub format: Option<OutputFormat>,
    pub tools: Option<Vec<Tool>>,
    pub options: Option<ModelOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputFormat {
    String(String),
    Schema(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
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
pub struct ModelOptions {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub num_ctx: Option<i32>,
    pub num_gpu: Option<i32>,
    pub repeat_penalty: Option<f32>,
    pub seed: Option<i32>,
    pub stop: Option<Vec<String>>,
    pub tfs_z: Option<f32>,
    pub num_predict: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: ChatMessage,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<i32>,
    pub prompt_eval_duration: Option<u64>,
    pub eval_count: Option<i32>,
    pub eval_duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<OllamaModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub model: Option<String>,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

impl Ollama {
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder().build().expect("failed to build reqwest client");
        Self {
            client,
            config,
            cached_models: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn chat(&self, request: &ChatRequest) -> LlmResult<ChatResponse> {
        let url = format!("{}/api/chat", self.config.base_url);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Provider {
                provider: "ollama".to_string(),
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
        let url = format!("{}/api/tags", self.config.base_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body = response.text().await?;
        let parsed: TagsResponse = serde_json::from_str(&body)
            .map_err(|e| Error::parse(body, e))?;

        Ok(parsed.models.into_iter().map(|m| Model::new(m.name)).collect())
    }
}

#[async_trait]
impl LlmProvider for Ollama {
    fn provider_name(&self) -> &'static str {
        "ollama"
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
