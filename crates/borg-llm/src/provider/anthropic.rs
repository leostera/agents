use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::completion::{
    FinishReason, Message, ModelSelector, ProviderType, RawCompletionRequest,
    RawCompletionResponse, Role, Usage as CompletionUsage,
};
use crate::error::{AnthropicConfigError, Error, LlmResult};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::tools::{RawToolCall, RawToolDefinition};
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub version: String,
    pub base_url: String,
    pub default_model: String,
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
            default_model: "claude-sonnet-4-5-20250520".to_string(),
        })
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
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
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    #[serde(rename_all = "camelCase")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
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
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Anthropic {
    pub fn new(config: AnthropicConfig) -> Self {
        let client = Client::builder()
            .build()
            .expect("failed to build reqwest client");
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
        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for Anthropic {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

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

        *cache = Some(Vec::new());
        Ok(Vec::new())
    }

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        let messages: Vec<crate::provider::anthropic::ChatMessage> = req
            .messages
            .iter()
            .map(|m| crate::provider::anthropic::ChatMessage {
                role: match m.role {
                    Role::System => crate::provider::anthropic::ChatRole::System,
                    Role::User => crate::provider::anthropic::ChatRole::User,
                    Role::Assistant => crate::provider::anthropic::ChatRole::Assistant,
                    Role::Tool => crate::provider::anthropic::ChatRole::User,
                },
                content: crate::provider::anthropic::Content::Text(m.content.clone()),
            })
            .collect();

        let system = req
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| crate::provider::anthropic::Content::Text(m.content.clone()));

        let chat_req = crate::provider::anthropic::ChatRequest {
            model: model.clone(),
            messages,
            system,
            max_tokens: req.max_tokens.unwrap_or(1024),
            temperature: req.temperature,
            top_p: req.top_p,
            top_k: None,
            tools: req.tools.map(map_tool_definitions),
            stop_sequences: None,
            stream: req.stream,
        };

        let chat_res = self.chat(&chat_req).await?;

        let text = chat_res
            .content
            .iter()
            .filter_map(|c| {
                if let crate::provider::anthropic::ResponseContentBlock::Text { text } = c {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        let tool_calls = chat_res
            .content
            .iter()
            .filter_map(|content| match content {
                crate::provider::anthropic::ResponseContentBlock::ToolUse { id, name, input } => {
                    Some(RawToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    })
                }
                crate::provider::anthropic::ResponseContentBlock::Text { .. } => None,
            })
            .collect::<Vec<_>>();

        Ok(RawCompletionResponse {
            provider: ProviderType::Anthropic,
            model: chat_res.model,
            message: Message {
                role: Role::Assistant,
                content: text,
            },
            tool_calls,
            usage: CompletionUsage {
                prompt_tokens: chat_res.usage.input_tokens,
                completion_tokens: chat_res.usage.output_tokens,
                total_tokens: chat_res.usage.input_tokens + chat_res.usage.output_tokens,
            },
            finish_reason: match chat_res.stop_reason.as_deref() {
                Some("tool_use") => FinishReason::ToolCalls,
                other => FinishReason::from(other.map(|value| value.to_string())),
            },
        })
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::NoMatchingProvider {
            reason: "Anthropic does not support audio transcription".to_string(),
        })
    }
}

fn map_tool_definitions(tools: Vec<RawToolDefinition>) -> Vec<ToolDefinition> {
    tools
        .into_iter()
        .map(|tool| ToolDefinition {
            name: tool.function.name,
            description: tool.function.description,
            input_schema: tool.function.parameters,
        })
        .collect()
}
