use async_trait::async_trait;
use derive_builder::Builder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::capability::Capability;
use crate::completion::{
    FinishReason, Message, ModelSelector, ProviderType, RawCompletionEvent,
    RawCompletionEventStream, RawCompletionRequest, RawCompletionResponse, Role, Usage,
};
use crate::error::{Error, LlmResult};
use crate::model::Model;
use crate::provider::LlmProvider;
use crate::tools::{RawToolCall, RawToolDefinition};
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub default_model: String,
}

impl OllamaConfig {
    pub fn new(default_model: impl Into<String>) -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
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
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self::new(String::new())
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
#[serde(rename_all = "snake_case")]
pub struct ModelOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_gpu: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tfs_z: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let parsed: ChatResponse = parse_chat_response_body(&body)?;
        Ok(parsed)
    }

    fn build_chat_request(&self, req: RawCompletionRequest) -> LlmResult<(String, ChatRequest)> {
        let model = match req.model {
            ModelSelector::Any => self.config.default_model.clone(),
            ModelSelector::Provider(_) => self.config.default_model.clone(),
            ModelSelector::Specific { model, .. } => model,
        };

        if model.is_empty() {
            return Err(Error::NoMatchingProvider {
                reason: "Ollama requires a model to be specified".to_string(),
            });
        }

        let messages: Vec<crate::provider::ollama::ChatMessage> = req
            .messages
            .iter()
            .map(|m| crate::provider::ollama::ChatMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "user".to_string(),
                },
                content: m.content.clone(),
                images: None,
                tool_calls: None,
            })
            .collect();

        let chat_req = crate::provider::ollama::ChatRequest {
            model: model.clone(),
            messages,
            stream: Some(req.response_mode.is_streaming()),
            format: req.response_format.and_then(|format| {
                format
                    .json_schema
                    .map(|schema| OutputFormat::Schema(schema.schema))
            }),
            tools: req.tools.map(map_tool_definitions),
            options: Some(ModelOptions {
                temperature: req.temperature.as_option(),
                top_p: req.top_p.as_option(),
                top_k: req.top_k.as_option_i32(),
                num_ctx: req
                    .token_limit
                    .as_option()
                    .and_then(|value| i32::try_from(value).ok()),
                num_gpu: None,
                repeat_penalty: None,
                seed: None,
                stop: None,
                tfs_z: None,
                num_predict: req
                    .token_limit
                    .as_option()
                    .and_then(|value| i32::try_from(value).ok()),
            }),
        };

        Ok((model, chat_req))
    }

    pub async fn list_models(&self) -> LlmResult<Vec<Model>> {
        let url = format!("{}/api/tags", self.config.base_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body = response.text().await?;
        let parsed: TagsResponse =
            serde_json::from_str(&body).map_err(|e| Error::parse(body, e))?;

        Ok(parsed
            .models
            .into_iter()
            .map(|m| Model::new(m.name))
            .collect())
    }
}

#[async_trait]
impl LlmProvider for Ollama {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }

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

    async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
        let (_, mut chat_req) = self.build_chat_request(req)?;
        chat_req.stream = Some(false);

        let chat_res = self.chat(&chat_req).await?;
        Ok(raw_response_from_chat(chat_res))
    }

    async fn chat_raw_stream(
        &self,
        req: RawCompletionRequest,
    ) -> LlmResult<RawCompletionEventStream> {
        let (_, mut chat_req) = self.build_chat_request(req)?;
        chat_req.stream = Some(true);

        let url = format!("{}/api/chat", self.config.base_url);
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&chat_req)
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

        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut response = response;
            let mut buffer = Vec::new();
            let mut content = String::new();
            let mut tool_calls = Vec::new();
            let mut seen_tool_calls = HashSet::new();
            let mut final_chunk: Option<ChatResponse> = None;

            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        buffer.extend_from_slice(&chunk);

                        while let Some(line) = take_next_json_line(&mut buffer) {
                            if line.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<ChatResponse>(&line) {
                                Ok(chunk) => {
                                    if emit_chunk_events(
                                        &chunk,
                                        &sender,
                                        &mut content,
                                        &mut tool_calls,
                                        &mut seen_tool_calls,
                                    )
                                    .await
                                    .is_err()
                                    {
                                        return;
                                    }
                                    final_chunk = Some(chunk);
                                }
                                Err(error) => {
                                    let _ = sender.send(Err(Error::parse(line, error))).await;
                                    return;
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(Error::Http { source: error })).await;
                        return;
                    }
                }
            }

            if !buffer.is_empty() {
                let line = String::from_utf8_lossy(&buffer).trim().to_string();
                if !line.is_empty() {
                    match serde_json::from_str::<ChatResponse>(&line) {
                        Ok(chunk) => {
                            if emit_chunk_events(
                                &chunk,
                                &sender,
                                &mut content,
                                &mut tool_calls,
                                &mut seen_tool_calls,
                            )
                            .await
                            .is_err()
                            {
                                return;
                            }
                            final_chunk = Some(chunk);
                        }
                        Err(error) => {
                            let _ = sender.send(Err(Error::parse(line, error))).await;
                            return;
                        }
                    }
                }
            }

            let Some(mut final_chunk) = final_chunk else {
                let _ = sender
                    .send(Err(Error::InvalidResponse {
                        reason: "Ollama returned an empty response stream".to_string(),
                    }))
                    .await;
                return;
            };

            final_chunk.message.content = content;
            final_chunk.message.tool_calls = if tool_calls.is_empty() {
                None
            } else {
                Some(
                    tool_calls
                        .iter()
                        .cloned()
                        .map(|call| ToolCall {
                            function: ToolCallFunction {
                                name: call.name,
                                arguments: call.arguments,
                            },
                        })
                        .collect(),
                )
            };

            let _ = sender
                .send(Ok(RawCompletionEvent::Done(raw_response_from_chat(
                    final_chunk,
                ))))
                .await;
        });

        Ok(RawCompletionEventStream::new(receiver))
    }

    async fn transcribe(
        &self,
        _req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        Err(Error::NoMatchingProvider {
            reason: "Ollama does not support audio transcription".to_string(),
        })
    }
}

fn map_tool_definitions(tools: Vec<RawToolDefinition>) -> Vec<Tool> {
    tools
        .into_iter()
        .map(|tool| Tool {
            r#type: tool.r#type,
            function: ToolFunction {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            },
        })
        .collect()
}

fn raw_response_from_chat(chat_res: ChatResponse) -> RawCompletionResponse {
    let tool_calls = chat_res
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|call| RawToolCall {
            id: call.function.name.clone(),
            name: call.function.name,
            arguments: call.function.arguments,
        })
        .collect::<Vec<_>>();
    let has_tool_calls = !tool_calls.is_empty();

    RawCompletionResponse {
        provider: ProviderType::Ollama,
        model: chat_res.model,
        message: Message {
            role: Role::Assistant,
            content: chat_res.message.content,
        },
        tool_calls,
        usage: Usage {
            prompt_tokens: chat_res.prompt_eval_count.unwrap_or(0) as u32,
            completion_tokens: chat_res.eval_count.unwrap_or(0) as u32,
            total_tokens: (chat_res.prompt_eval_count.unwrap_or(0)
                + chat_res.eval_count.unwrap_or(0)) as u32,
        },
        finish_reason: if chat_res.done {
            if has_tool_calls {
                FinishReason::ToolCalls
            } else {
                FinishReason::Stop
            }
        } else {
            FinishReason::Unknown("incomplete".to_string())
        },
    }
}

fn take_next_json_line(buffer: &mut Vec<u8>) -> Option<String> {
    let newline_index = buffer.iter().position(|byte| *byte == b'\n')?;
    let line = String::from_utf8_lossy(&buffer[..newline_index])
        .trim()
        .to_string();
    buffer.drain(..=newline_index);
    if line.is_empty() {
        return Some(String::new());
    }
    Some(line)
}

async fn emit_chunk_events(
    chunk: &ChatResponse,
    sender: &mpsc::Sender<LlmResult<RawCompletionEvent>>,
    content: &mut String,
    tool_calls: &mut Vec<RawToolCall>,
    seen_tool_calls: &mut HashSet<String>,
) -> Result<(), ()> {
    if !chunk.message.content.is_empty() {
        content.push_str(&chunk.message.content);
        if sender
            .send(Ok(RawCompletionEvent::TextDelta {
                text: chunk.message.content.clone(),
            }))
            .await
            .is_err()
        {
            return Err(());
        }
    }

    if let Some(calls) = &chunk.message.tool_calls {
        for call in calls {
            let raw_call = RawToolCall {
                id: call.function.name.clone(),
                name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            };
            let key = format!(
                "{}:{}",
                raw_call.name,
                serde_json::to_string(&raw_call.arguments).unwrap_or_default()
            );
            if !seen_tool_calls.insert(key) {
                continue;
            }
            tool_calls.push(raw_call.clone());
            if sender
                .send(Ok(RawCompletionEvent::ToolCall { call: raw_call }))
                .await
                .is_err()
            {
                return Err(());
            }
        }
    }

    Ok(())
}

fn parse_chat_response_body(body: &str) -> LlmResult<ChatResponse> {
    if let Ok(parsed) = serde_json::from_str::<ChatResponse>(body) {
        return Ok(parsed);
    }

    let mut chunks = Vec::new();
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        let chunk = serde_json::from_str::<ChatResponse>(line)
            .map_err(|e| Error::parse(body.to_string(), e))?;
        chunks.push(chunk);
    }

    if chunks.is_empty() {
        return Err(Error::InvalidResponse {
            reason: "Ollama returned an empty response body".to_string(),
        });
    }

    let first = chunks.first().cloned().ok_or(Error::InvalidResponse {
        reason: "Ollama returned no response chunks".to_string(),
    })?;
    let last = chunks.last().cloned().ok_or(Error::InvalidResponse {
        reason: "Ollama returned no response chunks".to_string(),
    })?;

    let mut content = String::new();
    let mut tool_calls: Option<Vec<ToolCall>> = None;
    for chunk in &chunks {
        content.push_str(&chunk.message.content);
        if let Some(calls) = &chunk.message.tool_calls {
            tool_calls = Some(calls.clone());
        }
    }

    Ok(ChatResponse {
        model: last.model,
        created_at: last.created_at,
        message: ChatMessage {
            role: first.message.role,
            content,
            images: None,
            tool_calls,
        },
        done: last.done,
        total_duration: last.total_duration,
        load_duration: last.load_duration,
        prompt_eval_count: last.prompt_eval_count,
        prompt_eval_duration: last.prompt_eval_duration,
        eval_count: last.eval_count,
        eval_duration: last.eval_duration,
    })
}
