use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::llm::capability::Capability;
use crate::llm::completion::{
    FinishReason, ModelSelector, ProviderType, RawCompletionEvent, RawCompletionEventStream,
    RawCompletionRequest, RawCompletionResponse, RawInputContent, RawInputItem, RawOutputContent,
    RawOutputItem, Role, Usage as CompletionUsage,
};
use crate::llm::error::{AnthropicConfigError, Error, LlmResult};
use crate::llm::model::Model;
use crate::llm::provider::LlmProvider;
use crate::llm::tools::{RawToolCall, RawToolDefinition};
use crate::llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub version: String,
    pub base_url: String,
    pub default_model: String,
}

impl AnthropicConfig {
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self, AnthropicConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(AnthropicConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            version: "2023-06-01".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            default_model: default_model.into(),
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
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Map<String, Value>,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Content>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResponseContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStartEvent {
    pub message: StreamMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessage {
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockDeltaEvent {
    #[serde(default)]
    pub index: u64,
    pub delta: ContentBlockDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaEvent {
    pub delta: MessageDelta,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlockStartEvent {
    #[serde(default)]
    pub index: u64,
    pub content_block: StreamContentBlock,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum StreamContentBlock {
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Option<Value>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlockStopEvent {
    #[serde(default)]
    pub index: u64,
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

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
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

        let system = req.input.iter().find_map(|item| match item {
            RawInputItem::Message {
                role: Role::System,
                content,
            } => Some(build_content(content)),
            RawInputItem::Message { .. }
            | RawInputItem::ToolCall { .. }
            | RawInputItem::ToolResult { .. } => None,
        });

        let messages: Vec<crate::llm::provider::anthropic::ChatMessage> = req
            .input
            .iter()
            .filter_map(|item| match item {
                RawInputItem::Message { role, content } => {
                    if *role == Role::System {
                        None
                    } else {
                        Some(crate::llm::provider::anthropic::ChatMessage {
                            role: match role {
                                Role::User => crate::llm::provider::anthropic::ChatRole::User,
                                Role::Assistant => {
                                    crate::llm::provider::anthropic::ChatRole::Assistant
                                }
                                Role::System => unreachable!(),
                            },
                            content: build_content(content),
                        })
                    }
                }
                RawInputItem::ToolCall { call } => {
                    Some(crate::llm::provider::anthropic::ChatMessage {
                        role: crate::llm::provider::anthropic::ChatRole::Assistant,
                        content: Content::Blocks(vec![ContentBlock::ToolUse {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            input: normalize_tool_use_input(call.arguments.clone()),
                        }]),
                    })
                }
                RawInputItem::ToolResult {
                    tool_use_id,
                    content,
                } => Some(crate::llm::provider::anthropic::ChatMessage {
                    role: crate::llm::provider::anthropic::ChatRole::User,
                    content: Content::Blocks(vec![ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                    }]),
                }),
            })
            .collect();

        let chat_req = crate::llm::provider::anthropic::ChatRequest {
            model: model.clone(),
            messages,
            system,
            max_tokens: req.token_limit.as_option().unwrap_or(1024),
            temperature: req.temperature.as_option(),
            top_p: req.top_p.as_option(),
            top_k: req.top_k.as_option_i32(),
            tools: req.tools.map(map_tool_definitions),
            stop_sequences: None,
            stream: Some(req.response_mode.is_streaming()),
        };

        let chat_res = self.chat(&chat_req).await?;

        let text = chat_res
            .content
            .iter()
            .filter_map(|c| {
                if let crate::llm::provider::anthropic::ResponseContentBlock::Text { text } = c {
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
                crate::llm::provider::anthropic::ResponseContentBlock::ToolUse {
                    id,
                    name,
                    input,
                } => Some(RawToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                }),
                crate::llm::provider::anthropic::ResponseContentBlock::Text { .. } => None,
            })
            .collect::<Vec<_>>();

        Ok(RawCompletionResponse {
            provider: ProviderType::Anthropic,
            model: chat_res.model,
            output: raw_output_from_anthropic(text, tool_calls),
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

    #[cfg(not(target_arch = "wasm32"))]
    async fn chat_raw_stream(
        &self,
        req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        let system = req.input.iter().find_map(|item| match item {
            RawInputItem::Message {
                role: Role::System,
                content,
            } => Some(build_content(content)),
            RawInputItem::Message { .. }
            | RawInputItem::ToolCall { .. }
            | RawInputItem::ToolResult { .. } => None,
        });

        let messages: Vec<crate::llm::provider::anthropic::ChatMessage> = req
            .input
            .iter()
            .filter_map(|item| match item {
                RawInputItem::Message { role, content } => {
                    if *role == Role::System {
                        None
                    } else {
                        Some(crate::llm::provider::anthropic::ChatMessage {
                            role: match role {
                                Role::User => crate::llm::provider::anthropic::ChatRole::User,
                                Role::Assistant => {
                                    crate::llm::provider::anthropic::ChatRole::Assistant
                                }
                                Role::System => unreachable!(),
                            },
                            content: build_content(content),
                        })
                    }
                }
                RawInputItem::ToolCall { call } => {
                    Some(crate::llm::provider::anthropic::ChatMessage {
                        role: crate::llm::provider::anthropic::ChatRole::Assistant,
                        content: Content::Blocks(vec![ContentBlock::ToolUse {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            input: normalize_tool_use_input(call.arguments.clone()),
                        }]),
                    })
                }
                RawInputItem::ToolResult {
                    tool_use_id,
                    content,
                } => Some(crate::llm::provider::anthropic::ChatMessage {
                    role: crate::llm::provider::anthropic::ChatRole::User,
                    content: Content::Blocks(vec![ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                    }]),
                }),
            })
            .collect();

        let chat_req = crate::llm::provider::anthropic::ChatRequest {
            model,
            messages,
            system,
            max_tokens: req.token_limit.as_option().unwrap_or(1024),
            temperature: req.temperature.as_option(),
            top_p: req.top_p.as_option(),
            top_k: req.top_k.as_option_i32(),
            tools: req.tools.map(map_tool_definitions),
            stop_sequences: None,
            stream: Some(true),
        };

        let url = format!("{}/v1/messages", self.config.base_url);
        let event_source = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", &self.config.version)
            .header("content-type", "application/json")
            .json(&chat_req)
            .eventsource()
            .map_err(|error| Error::from_eventsource_builder("anthropic", error))?;

        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut event_source = event_source;
            let mut model = None;
            let mut content = String::new();
            let mut tool_calls = Vec::new();
            let mut pending_tool_calls: std::collections::HashMap<u64, (String, String, String)> =
                std::collections::HashMap::new();
            let mut usage = Usage {
                input_tokens: 0,
                output_tokens: 0,
            };
            let mut finish_reason = FinishReason::Unknown("stream_incomplete".to_string());

            while let Some(event) = event_source.next().await {
                match event {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(message)) => match message.event.as_str() {
                        "message_start" => {
                            if let Ok(start) =
                                serde_json::from_str::<MessageStartEvent>(&message.data)
                            {
                                model = Some(start.message.model);
                                usage.input_tokens = start.message.usage.input_tokens;
                                usage.output_tokens = start.message.usage.output_tokens;
                            }
                        }
                        "content_block_delta" => {
                            match serde_json::from_str::<ContentBlockDeltaEvent>(&message.data) {
                                Ok(delta) => match delta.delta {
                                    ContentBlockDelta::TextDelta { text } => {
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
                                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                                        if let Some((_, _, input)) =
                                            pending_tool_calls.get_mut(&delta.index)
                                        {
                                            if input.trim() == "{}" {
                                                input.clear();
                                            }
                                            input.push_str(&partial_json);
                                        }
                                    }
                                },
                                Err(error) => {
                                    let _ =
                                        sender.send(Err(Error::parse(message.data, error))).await;
                                    let _ = event_source.close();
                                    return;
                                }
                            }
                        }
                        "content_block_start" => {
                            let parsed: ContentBlockStartEvent =
                                match serde_json::from_str(&message.data) {
                                    Ok(parsed) => parsed,
                                    Err(error) => {
                                        let _ = sender
                                            .send(Err(Error::parse(message.data, error)))
                                            .await;
                                        let _ = event_source.close();
                                        return;
                                    }
                                };
                            let index = parsed.index;
                            if let StreamContentBlock::ToolUse { id, name, input } =
                                parsed.content_block
                            {
                                let initial_input = input
                                    .map(|input| {
                                        if input.is_object() || input.is_array() {
                                            serde_json::to_string(&input).unwrap_or_default()
                                        } else {
                                            String::new()
                                        }
                                    })
                                    .unwrap_or_default();
                                pending_tool_calls.insert(index, (id, name, initial_input));
                            }
                        }
                        "content_block_stop" => {
                            let parsed: ContentBlockStopEvent =
                                match serde_json::from_str(&message.data) {
                                    Ok(parsed) => parsed,
                                    Err(error) => {
                                        let _ = sender
                                            .send(Err(Error::parse(message.data, error)))
                                            .await;
                                        let _ = event_source.close();
                                        return;
                                    }
                                };
                            let index = parsed.index;
                            if let Some((id, name, input)) = pending_tool_calls.remove(&index) {
                                let arguments = if input.trim().is_empty() {
                                    serde_json::json!({})
                                } else {
                                    match serde_json::from_str(&input) {
                                        Ok(arguments) => arguments,
                                        Err(error) => {
                                            let _ =
                                                sender.send(Err(Error::parse(input, error))).await;
                                            let _ = event_source.close();
                                            return;
                                        }
                                    }
                                };
                                let call = RawToolCall {
                                    id,
                                    name,
                                    arguments,
                                };
                                tool_calls.push(call.clone());
                                if sender
                                    .send(Ok(RawCompletionEvent::ToolCall { call }))
                                    .await
                                    .is_err()
                                {
                                    let _ = event_source.close();
                                    return;
                                }
                            }
                        }
                        "message_delta" => {
                            if let Ok(delta) =
                                serde_json::from_str::<MessageDeltaEvent>(&message.data)
                            {
                                usage.output_tokens = delta.usage.output_tokens;
                                finish_reason = match delta.delta.stop_reason.as_deref() {
                                    Some("tool_use") => FinishReason::ToolCalls,
                                    other => {
                                        FinishReason::from(other.map(|value| value.to_string()))
                                    }
                                };
                            }
                        }
                        "message_stop" => break,
                        "ping" => {}
                        _ => {}
                    },
                    Err(error) => {
                        let _ = sender
                            .send(Err(Error::from_eventsource("anthropic", error)))
                            .await;
                        let _ = event_source.close();
                        return;
                    }
                }
            }

            let _ = event_source.close();
            let _ = sender
                .send(Ok(RawCompletionEvent::Done(RawCompletionResponse {
                    provider: ProviderType::Anthropic,
                    model: model.unwrap_or_else(|| "unknown".to_string()),
                    output: raw_output_from_anthropic(content, tool_calls),
                    usage: CompletionUsage {
                        prompt_tokens: usage.input_tokens,
                        completion_tokens: usage.output_tokens,
                        total_tokens: usage.input_tokens + usage.output_tokens,
                    },
                    finish_reason,
                })))
                .await;
        });

        Ok(RawCompletionEventStream::new(receiver))
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

fn build_content(content: &[RawInputContent]) -> Content {
    let blocks = content
        .iter()
        .map(|content| match content {
            RawInputContent::Text { text } => ContentBlock::Text { text: text.clone() },
            RawInputContent::ImageUrl { url } => ContentBlock::Image {
                source: ImageSource::Base64 {
                    media_type: "text/plain".to_string(),
                    data: url.clone(),
                },
            },
        })
        .collect::<Vec<_>>();

    match blocks.as_slice() {
        [ContentBlock::Text { text }] => Content::Text(text.clone()),
        _ => Content::Blocks(blocks),
    }
}

fn raw_output_from_anthropic(text: String, tool_calls: Vec<RawToolCall>) -> Vec<RawOutputItem> {
    let mut output = Vec::new();
    if !text.is_empty() {
        output.push(RawOutputItem::Message {
            role: Role::Assistant,
            content: vec![RawOutputContent::Text { text }],
        });
    }
    output.extend(
        tool_calls
            .into_iter()
            .map(|call| RawOutputItem::ToolCall { call }),
    );
    output
}

fn normalize_tool_use_input(input: Value) -> Map<String, Value> {
    match input {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => {
            let mut map = Map::new();
            map.insert("_input".to_string(), other);
            map
        }
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
