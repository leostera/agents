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
use crate::error::{Error, LlmResult, OpenRouterConfigError};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::response::RawResponseFormat;
use crate::tools::{RawToolCall, RawToolDefinition};
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
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
            default_model: "openai/o4-mini".to_string(),
        })
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
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
    pub content: Option<String>,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolCall {
    pub id: String,
    pub r#type: String,
    pub function: ChatToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolCallFunction {
    pub name: String,
    pub arguments: String,
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
    Message {
        role: String,
        content: Vec<InputContent>,
    },
    Text {
        text: String,
    },
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
    Message {
        id: String,
        status: String,
        role: String,
        content: Vec<OutputContent>,
    },
    Reasoning {
        id: String,
        reasoning: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum OutputContent {
    OutputText {
        text: String,
        annotations: Vec<serde_json::Value>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
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
        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
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
        let parsed: ResponsesResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
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
        let parsed: CreditsResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for OpenRouter {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenRouter
    }

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
        *cache = Some(Vec::new());
        Ok(Vec::new())
    }

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        let messages: Vec<crate::provider::openrouter::ChatMessage> = req
            .messages
            .iter()
            .map(|m| crate::provider::openrouter::ChatMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "user".to_string(),
                },
                content: Some(m.content.clone()),
                name: None,
                tool_calls: None,
            })
            .collect();

        let chat_req = crate::provider::openrouter::ChatRequest {
            model: model.clone(),
            messages,
            temperature: req.temperature,
            top_p: req.top_p,
            max_tokens: req.max_tokens,
            stream: req.stream,
            stop: None,
            tools: req.tools.map(map_tool_definitions),
            tool_choice: req.tool_choice.map(|choice| ToolChoice {
                r#type: choice.r#type,
                function: choice.function.map(|function| ToolChoiceFunction {
                    name: function.name,
                }),
            }),
            response_format: req.response_format.map(map_response_format),
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            logit_bias: None,
        };

        let chat_res = self.chat(&chat_req).await?;
        let first_choice = chat_res.choices.first().ok_or(Error::InvalidResponse {
            reason: "OpenRouter response had no choices".to_string(),
        })?;
        let tool_calls = first_choice
            .message
            .tool_calls
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                Ok(RawToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: serde_json::from_str(&call.function.arguments)
                        .map_err(|e| Error::parse("tool arguments", e))?,
                })
            })
            .collect::<LlmResult<Vec<_>>>()?;

        Ok(RawCompletionResponse {
            provider: ProviderType::OpenRouter,
            model: chat_res.model,
            message: Message {
                role: Role::Assistant,
                content: first_choice.message.content.clone().unwrap_or_default(),
            },
            tool_calls,
            usage: CompletionUsage {
                prompt_tokens: chat_res.usage.prompt_tokens,
                completion_tokens: chat_res.usage.completion_tokens,
                total_tokens: chat_res.usage.total_tokens,
            },
            finish_reason: FinishReason::from(first_choice.finish_reason.clone()),
        })
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::NoMatchingProvider {
            reason: "OpenRouter does not support audio transcription".to_string(),
        })
    }
}

fn map_tool_definitions(tools: Vec<RawToolDefinition>) -> Vec<ToolDefinition> {
    tools
        .into_iter()
        .map(|tool| ToolDefinition {
            r#type: tool.r#type,
            function: ToolFunction {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            },
        })
        .collect()
}

fn map_response_format(format: RawResponseFormat) -> ResponseFormat {
    ResponseFormat {
        r#type: format.r#type,
        json_schema: format.json_schema.map(|schema| JsonSchema {
            name: schema.name,
            strict: schema.strict,
            schema: schema.schema,
        }),
    }
}
