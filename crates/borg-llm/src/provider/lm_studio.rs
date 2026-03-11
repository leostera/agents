use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capability::Capability;
use crate::completion::{
    FinishReason, ModelSelector, ProviderType, RawCompletionRequest, RawCompletionResponse,
    RawInputContent, RawInputItem, RawOutputContent, RawOutputItem, Role,
    ToolChoice as RawToolChoice, Usage as CompletionUsage,
};
use crate::error::{Error, LlmResult};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::response::RawResponseFormat;
use crate::tools::{RawToolCall, RawToolDefinition};
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct LmStudioConfig {
    pub base_url: String,
    pub api_token: Option<String>,
    pub default_model: String,
}

impl LmStudioConfig {
    pub fn new(default_model: impl Into<String>) -> Self {
        Self {
            base_url: "http://localhost:1234".to_string(),
            api_token: None,
            default_model: default_model.into(),
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
        Self::new(String::new())
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
    pub content: Option<String>,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
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
        let parsed: ModelsResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;

        Ok(parsed.data.into_iter().map(|m| Model::new(m.id)).collect())
    }
}

#[async_trait]
impl LlmProvider for LmStudio {
    fn provider_type(&self) -> ProviderType {
        ProviderType::LmStudio
    }

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

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        if model.is_empty() {
            return Err(Error::NoMatchingProvider {
                reason: "LmStudio requires a model to be specified".to_string(),
            });
        }

        let messages: Vec<crate::provider::lm_studio::ChatMessage> = req
            .input
            .iter()
            .map(|item| crate::provider::lm_studio::ChatMessage {
                role: match item {
                    RawInputItem::Message { role, .. } => match role {
                        Role::System => "system".to_string(),
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                    },
                    RawInputItem::ToolCall { .. } => "assistant".to_string(),
                    RawInputItem::ToolResult { .. } => "tool".to_string(),
                },
                content: Some(match item {
                    RawInputItem::Message { content, .. } => flatten_lmstudio_content(content),
                    RawInputItem::ToolCall { .. } => String::new(),
                    RawInputItem::ToolResult { content, .. } => content.clone(),
                }),
                name: None,
                tool_call_id: match item {
                    RawInputItem::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
                    RawInputItem::Message { .. } | RawInputItem::ToolCall { .. } => None,
                },
                tool_calls: match item {
                    RawInputItem::ToolCall { call } => Some(vec![ChatToolCall {
                        id: call.id.clone(),
                        r#type: "function".to_string(),
                        function: ChatToolCallFunction {
                            name: call.name.clone(),
                            arguments: serde_json::to_string(&call.arguments)
                                .expect("raw tool call arguments serialize"),
                        },
                    }]),
                    RawInputItem::Message { .. } | RawInputItem::ToolResult { .. } => None,
                },
            })
            .collect();

        let chat_req = crate::provider::lm_studio::ChatRequest {
            model: model.clone(),
            messages,
            temperature: req.temperature.as_option(),
            top_p: req.top_p.as_option(),
            top_k: req.top_k.as_option_i32(),
            max_tokens: req.token_limit.as_option(),
            stream: Some(req.response_mode.is_streaming()),
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            repeat_penalty: None,
            seed: None,
            tools: req.tools.map(map_tool_definitions),
            tool_choice: map_tool_choice(req.tool_choice),
            response_format: req.response_format.map(map_response_format),
        };

        let chat_res = self.chat(&chat_req).await?;
        let first_choice = chat_res.choices.first().ok_or(Error::InvalidResponse {
            reason: "LM Studio response had no choices".to_string(),
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
            provider: ProviderType::LmStudio,
            model: chat_res.model,
            output: raw_output_from_lmstudio(first_choice.message.content.clone(), tool_calls),
            usage: CompletionUsage {
                prompt_tokens: chat_res.usage.prompt_tokens.unwrap_or(0),
                completion_tokens: chat_res.usage.completion_tokens.unwrap_or(0),
                total_tokens: chat_res.usage.total_tokens.unwrap_or(0),
            },
            finish_reason: FinishReason::from(first_choice.finish_reason.clone()),
        })
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::NoMatchingProvider {
            reason: "LmStudio does not support audio transcription".to_string(),
        })
    }
}

fn flatten_lmstudio_content(content: &[RawInputContent]) -> String {
    content
        .iter()
        .filter_map(|content| match content {
            RawInputContent::Text { text } => Some(text.as_str()),
            RawInputContent::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_output_from_lmstudio(
    content: Option<String>,
    tool_calls: Vec<RawToolCall>,
) -> Vec<RawOutputItem> {
    let mut output = Vec::new();
    if let Some(content) = content.filter(|content| !content.is_empty()) {
        output.push(RawOutputItem::Message {
            role: Role::Assistant,
            content: vec![RawOutputContent::Text { text: content }],
        });
    }
    output.extend(
        tool_calls
            .into_iter()
            .map(|call| RawOutputItem::ToolCall { call }),
    );
    output
}

fn map_tool_choice(choice: RawToolChoice) -> Option<ToolChoice> {
    match choice {
        RawToolChoice::ProviderDefault => None,
        RawToolChoice::Auto => Some(ToolChoice {
            r#type: "auto".to_string(),
            function: None,
        }),
        RawToolChoice::Required => Some(ToolChoice {
            r#type: "required".to_string(),
            function: None,
        }),
        RawToolChoice::Specific { name } => Some(ToolChoice {
            r#type: "function".to_string(),
            function: Some(ToolChoiceFunction { name }),
        }),
        RawToolChoice::None => Some(ToolChoice {
            r#type: "none".to_string(),
            function: None,
        }),
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
