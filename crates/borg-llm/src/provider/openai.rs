mod json_normalizer;

use async_trait::async_trait;
use derive_builder::Builder;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::capability::Capability;
use crate::completion::{
    FinishReason, ModelSelector, ProviderType, RawCompletionEvent, RawCompletionEventStream,
    RawCompletionRequest, RawCompletionResponse, RawInputContent, RawInputItem, RawOutputContent,
    RawOutputItem, Role, ToolChoice as RawToolChoice, Usage as CompletionUsage,
};
use crate::error::{Error, LlmResult, OpenAIConfigError};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::response::RawResponseFormat;
use crate::tools::{RawToolCall, RawToolDefinition};
use crate::transcription::{
    AudioSource, AudioTranscriptionRequest, AudioTranscriptionResponse, TranscriptionLanguage,
    TranscriptionPrompt,
};
use json_normalizer::normalize_openai_schema;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub organization: Option<String>,
    pub default_model: String,
}

impl OpenAIConfig {
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Result<Self, OpenAIConfigError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(OpenAIConfigError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            base_url: "https://api.openai.com".to_string(),
            organization: None,
            default_model: default_model.into(),
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
    pub content: Option<String>,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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
pub struct ChatStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
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
#[serde(rename_all = "snake_case")]
pub struct ResponsesRequest {
    pub model: String,
    pub input: Vec<ResponseInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponseTextConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResponseInputItem {
    Message {
        role: String,
        content: Vec<ResponseContent>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResponseContent {
    InputText { text: String },
    OutputText { text: String },
    InputImage { image_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseToolDefinition {
    pub r#type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
    pub strict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseTextConfig {
    pub format: ResponseTextFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResponseTextFormat {
    Text,
    JsonSchema {
        name: String,
        schema: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
    JsonObject,
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

#[derive(Debug, Clone, Deserialize)]
struct ResponseOutputTextDeltaEvent {
    delta: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseOutputItemEvent {
    item: ResponseOutputItem,
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseOutputItem {
    id: String,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseFunctionCallArgumentsDeltaEvent {
    item_id: String,
    delta: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ResponseCompletedEvent {
    response: Value,
}

impl OpenAI {
    pub fn new(config: OpenAIConfig) -> Self {
        let client = Client::builder()
            .build()
            .expect("failed to build reqwest client");
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
        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }

    pub async fn responses(&self, request: &ResponsesRequest) -> LlmResult<Value> {
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
        let parsed: Value = serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
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
        let parsed: Eval = serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
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
        let parsed: EvalListResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;
        Ok(parsed)
    }
}

#[async_trait]
impl LlmProvider for OpenAI {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }

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

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let responses_req = build_responses_request(&self.config.default_model, req)?;
        let response = self.responses(&responses_req).await?;
        parse_responses_response(response)
    }

    async fn chat_raw_stream(
        &self,
        req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        let responses_req = build_responses_request(&self.config.default_model, req)?;
        let url = format!("{}/v1/responses", self.config.base_url);
        let auth = self.auth_header();
        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json");

        if let Some(ref org) = self.config.organization {
            req_builder = req_builder.header("OpenAI-Organization", org);
        }

        let event_source = req_builder
            .json(&responses_req)
            .eventsource()
            .map_err(|error| Error::from_eventsource_builder("openai", error))?;

        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut event_source = event_source;
            let mut function_calls: std::collections::HashMap<String, (String, String, String)> =
                std::collections::HashMap::new();

            while let Some(event) = event_source.next().await {
                match event {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(message)) => match message.event.as_str() {
                        "response.output_text.delta" => {
                            let parsed: ResponseOutputTextDeltaEvent =
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
                            if sender
                                .send(Ok(RawCompletionEvent::TextDelta { text: parsed.delta }))
                                .await
                                .is_err()
                            {
                                let _ = event_source.close();
                                return;
                            }
                        }
                        "response.output_item.added" | "response.output_item.done" => {
                            let parsed: ResponseOutputItemEvent =
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
                            if parsed.item.item_type == "function_call" {
                                let item_id = parsed.item.id;
                                let call_id = parsed
                                    .item
                                    .call_id
                                    .clone()
                                    .unwrap_or_else(|| item_id.clone());
                                let name = parsed.item.name.unwrap_or_default();
                                let arguments = parsed.item.arguments.unwrap_or_default();
                                function_calls.insert(
                                    item_id.clone(),
                                    (call_id.clone(), name.clone(), arguments.clone()),
                                );
                                if message.event == "response.output_item.done" {
                                    match parse_function_call(&call_id, &name, &arguments) {
                                        Ok(call) => {
                                            if sender
                                                .send(Ok(RawCompletionEvent::ToolCall { call }))
                                                .await
                                                .is_err()
                                            {
                                                let _ = event_source.close();
                                                return;
                                            }
                                        }
                                        Err(error) => {
                                            let _ = sender.send(Err(error)).await;
                                            let _ = event_source.close();
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        "response.function_call_arguments.delta" => {
                            let parsed: ResponseFunctionCallArgumentsDeltaEvent =
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
                            if let Some((_, _, arguments)) = function_calls.get_mut(&parsed.item_id)
                            {
                                arguments.push_str(&parsed.delta);
                            }
                        }
                        "response.completed" => {
                            let parsed: ResponseCompletedEvent =
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
                            let final_response = match parse_responses_response(parsed.response) {
                                Ok(response) => response,
                                Err(error) => {
                                    let _ = sender.send(Err(error)).await;
                                    let _ = event_source.close();
                                    return;
                                }
                            };
                            let _ = sender
                                .send(Ok(RawCompletionEvent::Done(final_response)))
                                .await;
                            break;
                        }
                        "response.failed" => {
                            let _ = sender
                                .send(Err(Error::InvalidResponse {
                                    reason: format!("OpenAI stream failed: {}", message.data),
                                }))
                                .await;
                            let _ = event_source.close();
                            return;
                        }
                        _ => {}
                    },
                    Err(error) => {
                        let _ = sender
                            .send(Err(Error::from_eventsource("openai", error)))
                            .await;
                        let _ = event_source.close();
                        return;
                    }
                }
            }

            let _ = event_source.close();
        });

        Ok(RawCompletionEventStream::new(receiver))
    }

    async fn transcribe(
        &self,
        req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        let url = format!("{}/v1/audio/transcriptions", self.config.base_url);

        let model = match &req.model {
            ModelSelector::Any | ModelSelector::Provider(_) => "whisper-1".to_string(),
            ModelSelector::Specific { model, .. } => model.clone(),
        };

        let (audio_data, file_name, mime_type) = match &req.audio {
            AudioSource::Data(data) => (
                data.clone(),
                "audio.wav".to_string(),
                "audio/wav".to_string(),
            ),
            AudioSource::Url(_) => {
                return Err(Error::InvalidRequest {
                    reason: "URL audio not supported yet".to_string(),
                });
            }
            AudioSource::Path(path) => (
                std::fs::read(path).map_err(|e| Error::InvalidRequest {
                    reason: e.to_string(),
                })?,
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("audio")
                    .to_string(),
                match path.extension().and_then(|ext| ext.to_str()) {
                    Some("ogg") => "audio/ogg",
                    Some("mp3") => "audio/mpeg",
                    Some("m4a") => "audio/mp4",
                    Some("wav") => "audio/wav",
                    Some("webm") => "audio/webm",
                    Some("flac") => "audio/flac",
                    _ => "application/octet-stream",
                }
                .to_string(),
            ),
        };

        let part = reqwest::multipart::Part::bytes(audio_data)
            .file_name(file_name)
            .mime_str(&mime_type)
            .map_err(|e| Error::InvalidRequest {
                reason: e.to_string(),
            })?;

        let mut form = reqwest::multipart::Form::new()
            .text("model", model.clone())
            .part("file", part);

        if let TranscriptionLanguage::Explicit { language } = req.language {
            form = form.text("language", language);
        }

        if let TranscriptionPrompt::Hint { text } = req.prompt {
            form = form.text("prompt", text);
        }

        if let Some(response_format) = req.response_format.as_openai_str() {
            form = form.text("response_format", response_format.to_string());
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .multipart(form)
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
        #[derive(Deserialize)]
        struct TranscriptionResponse {
            text: String,
        }
        let parsed: TranscriptionResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;

        Ok(AudioTranscriptionResponse {
            provider: ProviderType::OpenAI,
            model,
            text: parsed.text,
        })
    }
}

fn build_responses_request(
    default_model: &str,
    req: RawCompletionRequest,
) -> LlmResult<ResponsesRequest> {
    let model = match req.model {
        ModelSelector::Any => default_model.to_string(),
        ModelSelector::Provider(_) => default_model.to_string(),
        ModelSelector::Specific { model, .. } => model,
    };

    let input = req
        .input
        .into_iter()
        .map(|item| -> LlmResult<ResponseInputItem> {
            Ok(match item {
                RawInputItem::Message { role, content } => ResponseInputItem::Message {
                    role: match role {
                        Role::System => "system".to_string(),
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                    },
                    content: content
                        .into_iter()
                        .map(|content| match content {
                            RawInputContent::Text { text } => match role {
                                Role::Assistant => ResponseContent::OutputText { text },
                                Role::System | Role::User => ResponseContent::InputText { text },
                            },
                            RawInputContent::ImageUrl { url } => {
                                ResponseContent::InputImage { image_url: url }
                            }
                        })
                        .collect(),
                },
                RawInputItem::ToolCall { call } => ResponseInputItem::FunctionCall {
                    call_id: call.id,
                    name: call.name,
                    arguments: serde_json::to_string(&call.arguments)
                        .map_err(|error| Error::parse("tool call arguments", error))?,
                },
                RawInputItem::ToolResult {
                    tool_use_id,
                    content,
                } => ResponseInputItem::FunctionCallOutput {
                    call_id: tool_use_id,
                    output: content,
                },
            })
        })
        .collect::<LlmResult<Vec<_>>>()?;

    Ok(ResponsesRequest {
        model,
        input,
        temperature: req.temperature.as_option(),
        top_p: req.top_p.as_option(),
        max_output_tokens: req.token_limit.as_option(),
        stream: Some(req.response_mode.is_streaming()),
        tools: req.tools.map(map_response_tools),
        tool_choice: map_responses_tool_choice(req.tool_choice),
        text: req.response_format.map(map_response_text_config),
    })
}

fn map_response_tools(tools: Vec<RawToolDefinition>) -> Vec<ResponseToolDefinition> {
    tools
        .into_iter()
        .map(|tool| ResponseToolDefinition {
            r#type: tool.r#type,
            name: tool.function.name,
            description: tool.function.description,
            parameters: normalize_openai_schema(tool.function.parameters),
            strict: true,
        })
        .collect()
}

fn map_responses_tool_choice(choice: RawToolChoice) -> Option<Value> {
    match choice {
        RawToolChoice::ProviderDefault => None,
        RawToolChoice::Auto => Some(json!("auto")),
        RawToolChoice::Required => Some(json!("required")),
        RawToolChoice::Specific { name } => Some(json!({
            "type": "function",
            "name": name,
        })),
        RawToolChoice::None => Some(json!("none")),
    }
}

fn map_response_text_config(format: RawResponseFormat) -> ResponseTextConfig {
    ResponseTextConfig {
        format: match format.json_schema {
            Some(schema) => ResponseTextFormat::JsonSchema {
                name: schema.name,
                schema: normalize_openai_schema(schema.schema),
                description: None,
                strict: schema.strict,
            },
            None if format.r#type == "json_object" => ResponseTextFormat::JsonObject,
            None => ResponseTextFormat::Text,
        },
    }
}

fn parse_responses_response(value: Value) -> LlmResult<RawCompletionResponse> {
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .ok_or(Error::InvalidResponse {
            reason: "OpenAI responses payload missing model".to_string(),
        })?
        .to_string();

    let output_values =
        value
            .get("output")
            .and_then(Value::as_array)
            .ok_or(Error::InvalidResponse {
                reason: "OpenAI responses payload missing output".to_string(),
            })?;

    let mut output = Vec::new();
    let mut saw_tool_call = false;

    for item in output_values {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                let mut content = Vec::new();
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for part in parts {
                        match part.get("type").and_then(Value::as_str) {
                            Some("output_text") => {
                                if let Some(text) = part.get("text").and_then(Value::as_str) {
                                    content.push(RawOutputContent::Text {
                                        text: text.to_string(),
                                    });
                                }
                            }
                            Some("output_json") => {
                                if let Some(json) = part.get("json") {
                                    content.push(RawOutputContent::Json {
                                        value: json.clone(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !content.is_empty() {
                    output.push(RawOutputItem::Message {
                        role: Role::Assistant,
                        content,
                    });
                }
            }
            Some("function_call") => {
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("id").and_then(Value::as_str))
                    .ok_or(Error::InvalidResponse {
                        reason: "OpenAI function_call missing call id".to_string(),
                    })?;
                let name =
                    item.get("name")
                        .and_then(Value::as_str)
                        .ok_or(Error::InvalidResponse {
                            reason: "OpenAI function_call missing name".to_string(),
                        })?;
                let arguments = item.get("arguments").and_then(Value::as_str).ok_or(
                    Error::InvalidResponse {
                        reason: "OpenAI function_call missing arguments".to_string(),
                    },
                )?;
                output.push(RawOutputItem::ToolCall {
                    call: parse_function_call(call_id, name, arguments)?,
                });
                saw_tool_call = true;
            }
            Some("reasoning") => {
                let summary = item
                    .get("summary")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !summary.is_empty() {
                    output.push(RawOutputItem::Reasoning { text: summary });
                }
            }
            _ => {}
        }
    }

    let usage = value.get("usage").cloned().unwrap_or_else(|| json!({}));
    let prompt_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let completion_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or((prompt_tokens + completion_tokens) as u64) as u32;

    Ok(RawCompletionResponse {
        provider: ProviderType::OpenAI,
        model,
        output,
        usage: CompletionUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        },
        finish_reason: if saw_tool_call {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        },
    })
}

fn parse_function_call(call_id: &str, name: &str, arguments: &str) -> LlmResult<RawToolCall> {
    Ok(RawToolCall {
        id: call_id.to_string(),
        name: name.to_string(),
        arguments: serde_json::from_str(arguments)
            .map_err(|e| Error::parse("tool arguments", e))?,
    })
}
