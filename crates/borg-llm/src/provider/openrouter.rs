use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::capability::Capability;
use crate::completion::{
    FinishReason, ModelSelector, ProviderType, RawCompletionEvent, RawCompletionEventStream,
    RawCompletionRequest, RawCompletionResponse, RawInputContent, RawInputItem, RawOutputContent,
    RawOutputItem, Role, ToolChoice as RawToolChoice, Usage as CompletionUsage,
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
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self, OpenRouterConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(OpenRouterConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            base_url: "https://openrouter.ai".to_string(),
            default_model: default_model.into(),
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

fn debug_openrouter_streaming() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("BORG_LLM_DEBUG_OPENROUTER_STREAM")
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                matches!(value.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
    })
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
#[serde(rename_all = "snake_case")]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct ChatStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamToolCallDelta {
    pub index: Option<u32>,
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub function: Option<StreamToolCallFunctionDelta>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamToolCallFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn apply_stream_tool_call_delta(
    partial_tool_calls: &mut std::collections::BTreeMap<u32, PartialToolCall>,
    call: StreamToolCallDelta,
) {
    let Some(index) = call.index else {
        return;
    };
    let partial = partial_tool_calls.entry(index).or_default();
    if let Some(id) = call.id {
        partial.id = id;
    }
    if let Some(function) = call.function {
        if let Some(name) = function.name {
            partial.name = name;
        }
        if let Some(arguments) = function.arguments {
            partial.arguments.push_str(&arguments);
        }
    }
}

fn finalize_partial_tool_calls(
    partial_tool_calls: std::collections::BTreeMap<u32, PartialToolCall>,
) -> LlmResult<Vec<RawToolCall>> {
    let mut tool_calls = Vec::new();
    for partial in partial_tool_calls.into_values() {
        if partial.name.is_empty() {
            continue;
        }
        let arguments = if partial.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&partial.arguments)
                .map_err(|error| Error::parse(partial.arguments, error))?
        };
        tool_calls.push(RawToolCall {
            id: if partial.id.is_empty() {
                partial.name.clone()
            } else {
                partial.id
            },
            name: partial.name,
            arguments,
        });
    }
    Ok(tool_calls)
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
            .input
            .iter()
            .map(|item| crate::provider::openrouter::ChatMessage {
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
                    RawInputItem::Message { content, .. } => flatten_openrouter_content(content),
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

        let chat_req = crate::provider::openrouter::ChatRequest {
            model: model.clone(),
            messages,
            temperature: req.temperature.as_option(),
            top_p: req.top_p.as_option(),
            max_tokens: req.token_limit.as_option(),
            stream: Some(req.response_mode.is_streaming()),
            stop: None,
            tools: req.tools.map(map_tool_definitions),
            tool_choice: map_tool_choice(req.tool_choice),
            response_format: req.response_format.map(map_response_format),
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            logit_bias: None,
        };

        let chat_res = self.chat(&chat_req).await?;
        raw_response_from_chat(chat_res)
    }

    async fn chat_raw_stream(
        &self,
        req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        let messages: Vec<crate::provider::openrouter::ChatMessage> = req
            .input
            .iter()
            .map(|item| crate::provider::openrouter::ChatMessage {
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
                    RawInputItem::Message { content, .. } => flatten_openrouter_content(content),
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

        let chat_req = crate::provider::openrouter::ChatRequest {
            model,
            messages,
            temperature: req.temperature.as_option(),
            top_p: req.top_p.as_option(),
            max_tokens: req.token_limit.as_option(),
            stream: Some(true),
            stop: None,
            tools: req.tools.map(map_tool_definitions),
            tool_choice: map_tool_choice(req.tool_choice),
            response_format: req.response_format.map(map_response_format),
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            logit_bias: None,
        };

        let url = format!("{}/api/v1/chat/completions", self.config.base_url);
        let event_source = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/leostera/borg")
            .header("X-Title", "Borg")
            .json(&chat_req)
            .eventsource()
            .map_err(|error| Error::from_eventsource_builder("openrouter", error))?;

        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut event_source = event_source;
            let mut model = None;
            let mut content = String::new();
            let mut tool_calls = Vec::new();
            let mut partial_tool_calls = std::collections::BTreeMap::<u32, PartialToolCall>::new();
            let mut finish_reason = FinishReason::Unknown("stream_incomplete".to_string());

            while let Some(event) = event_source.next().await {
                match event {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(message)) => {
                        if debug_openrouter_streaming() {
                            eprintln!(
                                "[openrouter-stream] event={} data={}",
                                message.event, message.data
                            );
                        }
                        if message.data == "[DONE]" {
                            break;
                        }

                        match serde_json::from_str::<ChatStreamChunk>(&message.data) {
                            Ok(chunk) => {
                                model = Some(chunk.model.clone());
                                if let Some(choice) = chunk.choices.first() {
                                    if let Some(text) = choice.delta.content.clone() {
                                        content.push_str(&text);
                                        if sender
                                            .send(Ok(RawCompletionEvent::TextDelta { text }))
                                            .await
                                            .is_err()
                                        {
                                            let _ = event_source.close();
                                            return;
                                        }
                                    }
                                    if let Some(delta_tool_calls) = choice.delta.tool_calls.clone()
                                    {
                                        for call in delta_tool_calls {
                                            apply_stream_tool_call_delta(
                                                &mut partial_tool_calls,
                                                call,
                                            );
                                        }
                                    }
                                    if choice.finish_reason.is_some() {
                                        finish_reason =
                                            FinishReason::from(choice.finish_reason.clone());
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = sender.send(Err(Error::parse(message.data, error))).await;
                                let _ = event_source.close();
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender
                            .send(Err(Error::from_eventsource("openrouter", error)))
                            .await;
                        let _ = event_source.close();
                        return;
                    }
                }
            }

            let _ = event_source.close();
            let finalized_tool_calls = match finalize_partial_tool_calls(partial_tool_calls) {
                Ok(tool_calls) => tool_calls,
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            };
            for raw_call in finalized_tool_calls {
                tool_calls.push(raw_call.clone());
                if sender
                    .send(Ok(RawCompletionEvent::ToolCall { call: raw_call }))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let final_response = RawCompletionResponse {
                provider: ProviderType::OpenRouter,
                model: model.unwrap_or_else(|| "unknown".to_string()),
                output: raw_output_from_openrouter(Some(content), tool_calls),
                usage: CompletionUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
                finish_reason,
            };
            let _ = sender
                .send(Ok(RawCompletionEvent::Done(final_response)))
                .await;
        });

        Ok(RawCompletionEventStream::new(receiver))
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

fn flatten_openrouter_content(content: &[RawInputContent]) -> String {
    content
        .iter()
        .filter_map(|content| match content {
            RawInputContent::Text { text } => Some(text.as_str()),
            RawInputContent::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_output_from_openrouter(
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

fn raw_response_from_chat(chat_res: ChatResponse) -> LlmResult<RawCompletionResponse> {
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
        output: raw_output_from_openrouter(first_choice.message.content.clone(), tool_calls),
        usage: CompletionUsage {
            prompt_tokens: chat_res.usage.prompt_tokens,
            completion_tokens: chat_res.usage.completion_tokens,
            total_tokens: chat_res.usage.total_tokens,
        },
        finish_reason: FinishReason::from(first_choice.finish_reason.clone()),
    })
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
