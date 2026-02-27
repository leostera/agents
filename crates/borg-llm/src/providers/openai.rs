use anyhow::{Result, anyhow};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info, trace};

use crate::providers::call_trace::ProviderCallTrace;
use crate::{
    AuthProvider, DeviceCodeAuthConfig, LlmAssistantMessage, LlmRequest, Provider, ProviderBlock,
    ProviderMessage, StopReason, ToolDescriptor, TranscriptionRequest, UserBlock,
};

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_COMPLETIONS_URL: &str = "https://api.openai.com/v1/completions";
const OPENAI_AUDIO_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const OPENAI_DEVICE_CODE_URL: &str = "https://auth.openai.com/oauth/device/code";
const OPENAI_DEVICE_CODE_SCOPE: &str = "openid profile email offline_access";

#[derive(Clone, Copy, Debug)]
pub enum OpenAiApiMode {
    ChatCompletions,
    Completions,
}

#[derive(Clone)]
pub struct OpenAiProvider {
    http: Client,
    api_key: String,
    api_mode: OpenAiApiMode,
    chat_completions_url: String,
    completions_url: String,
    audio_transcriptions_url: String,
    device_code_url: String,
    device_code_scope: String,
    default_chat_completions_model: Option<String>,
    default_audio_transcriptions_model: Option<String>,
}

impl OpenAiProvider {
    pub fn build() -> OpenAiProviderBuilder {
        OpenAiProviderBuilder::new()
    }

    pub fn new(api_key: impl Into<String>) -> Self {
        Self::new_with_mode(api_key, OpenAiApiMode::ChatCompletions)
    }

    pub fn new_with_mode(api_key: impl Into<String>, api_mode: OpenAiApiMode) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            api_mode,
            chat_completions_url: OPENAI_CHAT_COMPLETIONS_URL.to_string(),
            completions_url: OPENAI_COMPLETIONS_URL.to_string(),
            audio_transcriptions_url: OPENAI_AUDIO_TRANSCRIPTIONS_URL.to_string(),
            device_code_url: OPENAI_DEVICE_CODE_URL.to_string(),
            device_code_scope: OPENAI_DEVICE_CODE_SCOPE.to_string(),
            default_chat_completions_model: None,
            default_audio_transcriptions_model: None,
        }
    }

    pub fn new_with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self::new_with_base_url_and_mode(api_key, base_url, OpenAiApiMode::ChatCompletions)
    }

    pub fn new_with_base_url_and_mode(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        api_mode: OpenAiApiMode,
    ) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            api_mode,
            chat_completions_url: format!("{}/v1/chat/completions", base),
            completions_url: format!("{}/v1/completions", base),
            audio_transcriptions_url: format!("{}/v1/audio/transcriptions", base),
            device_code_url: OPENAI_DEVICE_CODE_URL.to_string(),
            device_code_scope: OPENAI_DEVICE_CODE_SCOPE.to_string(),
            default_chat_completions_model: None,
            default_audio_transcriptions_model: None,
        }
    }
}

pub struct OpenAiProviderBuilder {
    api_key: Option<String>,
    api_mode: OpenAiApiMode,
    base_url: Option<String>,
    device_code_url: Option<String>,
    device_code_scope: Option<String>,
    chat_completions_model: Option<String>,
    audio_transcriptions_model: Option<String>,
}

impl OpenAiProviderBuilder {
    pub fn new() -> Self {
        Self {
            api_key: None,
            api_mode: OpenAiApiMode::ChatCompletions,
            base_url: None,
            device_code_url: None,
            device_code_scope: None,
            chat_completions_model: None,
            audio_transcriptions_model: None,
        }
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn api_mode(mut self, api_mode: OpenAiApiMode) -> Self {
        self.api_mode = api_mode;
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn chat_completions_model(mut self, model: impl Into<String>) -> Self {
        self.chat_completions_model = Some(model.into());
        self
    }

    pub fn device_code_url(mut self, url: impl Into<String>) -> Self {
        self.device_code_url = Some(url.into());
        self
    }

    pub fn device_code_scope(mut self, scope: impl Into<String>) -> Self {
        self.device_code_scope = Some(scope.into());
        self
    }

    pub fn audio_transcriptions_model(mut self, model: impl Into<String>) -> Self {
        self.audio_transcriptions_model = Some(model.into());
        self
    }

    pub fn build(self) -> Result<OpenAiProvider> {
        let api_key = self
            .api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .ok_or_else(|| anyhow!("OpenAI api_key is required"))?;
        let mut provider = if let Some(base_url) = self.base_url {
            OpenAiProvider::new_with_base_url_and_mode(api_key, base_url, self.api_mode)
        } else {
            OpenAiProvider::new_with_mode(api_key, self.api_mode)
        };
        provider.device_code_url = normalize_optional(self.device_code_url)
            .unwrap_or_else(|| OPENAI_DEVICE_CODE_URL.to_string());
        provider.device_code_scope = normalize_optional(self.device_code_scope)
            .unwrap_or_else(|| OPENAI_DEVICE_CODE_SCOPE.to_string());
        provider.default_chat_completions_model = normalize_optional(self.chat_completions_model);
        provider.default_audio_transcriptions_model =
            normalize_optional(self.audio_transcriptions_model);
        Ok(provider)
    }
}

impl Default for OpenAiProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn provider_name(&self) -> &'static str {
        "openai"
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        match self.api_mode {
            OpenAiApiMode::ChatCompletions => self.chat_via_chat_completions(req).await,
            OpenAiApiMode::Completions => self.chat_via_completions(req).await,
        }
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        let model = req
            .model
            .as_ref()
            .and_then(|model| {
                let model = model.trim();
                if model.is_empty() {
                    None
                } else {
                    Some(model.to_string())
                }
            })
            .or_else(|| self.default_audio_transcriptions_model.clone())
            .ok_or_else(|| {
                anyhow!(
                    "audio transcription model is required (set request.model or configure audio_transcriptions_model on OpenAiProvider::build())"
                )
            })?;
        info!(
            target: "borg_llm",
            model = model.as_str(),
            mime_type = req.mime_type.as_str(),
            bytes = req.audio.len(),
            "sending audio transcription request"
        );
        let call = ProviderCallTrace::sent(
            "openai",
            "audio_transcription",
            model.clone(),
            json!({
                "endpoint": self.audio_transcriptions_url.as_str(),
                "mime_type": req.mime_type.as_str(),
                "language": req.language.clone(),
                "prompt": req.prompt.clone(),
                "model": model,
                "audio_base64": STANDARD.encode(&req.audio),
            }),
        );

        let file_name = transcription_filename_for_mime(&req.mime_type);
        let part = reqwest::multipart::Part::bytes(req.audio.clone())
            .file_name(file_name)
            .mime_str(&req.mime_type)?;
        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", model);
        if let Some(language) = req
            .language
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            form = form.text("language", language.clone());
        }
        if let Some(prompt) = req.prompt.as_ref().filter(|value| !value.trim().is_empty()) {
            form = form.text("prompt", prompt.clone());
        }

        let response = self
            .http
            .post(&self.audio_transcriptions_url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error_message = format!("openai audio transcriptions returned {}", status);
            call.failed(Some(status), None, Some(body.as_str()), &error_message);
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "audio transcription request failed"
            );
            return Err(anyhow!(error_message));
        }

        let payload: Value = response.json().await?;
        call.succeeded(status, &payload);
        let text = payload
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing text in openai transcription response"))?
            .to_string();
        debug!(target: "borg_llm", chars = text.len(), "audio transcription request succeeded");
        Ok(text)
    }
}

impl AuthProvider for OpenAiProvider {
    fn device_code_auth_config(&self) -> Option<DeviceCodeAuthConfig> {
        Some(DeviceCodeAuthConfig {
            url: self.device_code_url.clone(),
            scope: Some(self.device_code_scope.clone()),
        })
    }
}

impl OpenAiProvider {
    async fn chat_via_chat_completions(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let model = self.resolve_chat_model(req)?;
        info!(
            target: "borg_llm",
            model = model.as_str(),
            message_count = req.messages.len(),
            tool_count = req.tools.len(),
            "sending chat completion request"
        );
        let body = json!({
            "model": model,
            "messages": to_openai_messages(&req.messages),
            "tools": to_openai_tools(&req.tools),
            "tool_choice": "auto",
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });
        let call =
            ProviderCallTrace::sent("openai", "chat_completion", model.clone(), body.clone());
        trace!(
            target: "borg_llm",
            has_temperature = req.temperature.is_some(),
            has_max_tokens = req.max_tokens.is_some(),
            "chat request payload prepared"
        );

        let api_key = req.api_key.as_deref().unwrap_or(&self.api_key);
        debug!(
            target: "borg_llm",
            endpoint = self.chat_completions_url.as_str(),
            request_api_key_override = req.api_key.is_some(),
            "posting chat completion request"
        );
        let response = self
            .http
            .post(&self.chat_completions_url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error_message = format!("openai chat completions returned {}", status);
            call.failed(Some(status), None, Some(body.as_str()), &error_message);
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "chat completion request failed"
            );
            return Err(anyhow!(error_message));
        }

        debug!(target: "borg_llm", status = %status, "chat completion request succeeded");
        let payload: Value = response.json().await?;
        call.succeeded(status, &payload);
        trace!(target: "borg_llm", payload = ?payload, "raw chat completion payload");
        parse_openai_assistant_message(&payload)
    }

    async fn chat_via_completions(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let model = self.resolve_chat_model(req)?;
        info!(
            target: "borg_llm",
            model = model.as_str(),
            message_count = req.messages.len(),
            tool_count = req.tools.len(),
            "sending completions request"
        );
        let body = json!({
            "model": model,
            "prompt": to_openai_completions_prompt(&req.messages, &req.tools),
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });
        let call = ProviderCallTrace::sent("openai", "completions", model.clone(), body.clone());

        let api_key = req.api_key.as_deref().unwrap_or(&self.api_key);
        let response = self
            .http
            .post(&self.completions_url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error_message = format!("openai completions returned {}", status);
            call.failed(Some(status), None, Some(body.as_str()), &error_message);
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "completions request failed"
            );
            return Err(anyhow!(error_message));
        }

        let payload: Value = response.json().await?;
        call.succeeded(status, &payload);
        trace!(target: "borg_llm", payload = ?payload, "raw completions payload");
        let text = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let stop_reason = match payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
        {
            Some("length") => StopReason::Error,
            _ => StopReason::EndOfTurn,
        };
        Ok(LlmAssistantMessage {
            content: vec![ProviderBlock::Text(text)],
            stop_reason,
            error_message: None,
            usage_tokens: payload_usage_tokens(&payload),
        })
    }
}

impl OpenAiProvider {
    fn resolve_chat_model(&self, req: &LlmRequest) -> Result<String> {
        if !req.model.trim().is_empty() {
            return Ok(req.model.clone());
        }
        if let Some(default_model) = &self.default_chat_completions_model {
            return Ok(default_model.clone());
        }
        Err(anyhow!(
            "chat completion model is required (set request.model or configure chat_completions_model on OpenAiProvider::build())"
        ))
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
}

fn transcription_filename_for_mime(mime: &str) -> String {
    let extension = match mime {
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "mp4",
        "audio/wav" => "wav",
        "audio/webm" => "webm",
        _ => "bin",
    };
    format!("telegram_voice.{}", extension)
}

fn to_openai_tools(tools: &[ToolDescriptor]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })
        })
        .collect()
}

fn to_openai_messages(messages: &[ProviderMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| match message {
            ProviderMessage::System { text } => json!({
                "role": "system",
                "content": text
            }),
            ProviderMessage::User { content } => json!({
                "role": "user",
                "content": user_blocks_to_openai_content(content)
            }),
            ProviderMessage::Assistant { content } => {
                let text_content = blocks_to_openai_content(content);
                let tool_calls = blocks_to_openai_tool_calls(content);
                if tool_calls.is_empty() {
                    json!({
                        "role": "assistant",
                        "content": text_content
                    })
                } else {
                    json!({
                        "role": "assistant",
                        "content": text_content,
                        "tool_calls": tool_calls
                    })
                }
            }
            ProviderMessage::ToolResult {
                tool_call_id,
                name,
                content,
            } => json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "name": name,
                "content": blocks_to_openai_content(content)
            }),
        })
        .collect()
}

fn to_openai_completions_prompt(messages: &[ProviderMessage], tools: &[ToolDescriptor]) -> String {
    let mut prompt = String::new();

    if !tools.is_empty() {
        prompt.push_str("Available tools (function signatures in JSON Schema):\n");
        for tool in tools {
            prompt.push_str("- ");
            prompt.push_str(tool.name.as_str());
            prompt.push_str(": ");
            prompt.push_str(tool.description.as_str());
            prompt.push_str("\n  parameters: ");
            prompt.push_str(&tool.input_schema.to_string());
            prompt.push('\n');
        }
        prompt.push('\n');
    }

    for message in messages {
        match message {
            ProviderMessage::System { text } => {
                prompt.push_str("[system]\n");
                prompt.push_str(text);
                prompt.push('\n');
            }
            ProviderMessage::User { content } => {
                prompt.push_str("[user]\n");
                let line = user_blocks_to_openai_content(content)
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                prompt.push_str(line.as_str());
                prompt.push('\n');
            }
            ProviderMessage::Assistant { content } => {
                prompt.push_str("[assistant]\n");
                let line = blocks_to_openai_content(content)
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                prompt.push_str(line.as_str());
                prompt.push('\n');
            }
            ProviderMessage::ToolResult { name, content, .. } => {
                prompt.push_str("[tool_result:");
                prompt.push_str(name);
                prompt.push_str("]\n");
                let line = blocks_to_openai_content(content)
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                prompt.push_str(line.as_str());
                prompt.push('\n');
            }
        }
    }
    prompt.push_str("[assistant]\n");
    prompt
}

fn blocks_to_openai_tool_calls(blocks: &[ProviderBlock]) -> Vec<Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ProviderBlock::ToolCall {
                id,
                name,
                arguments_json,
            } => Some(json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments_json.to_string()
                }
            })),
            _ => None,
        })
        .collect()
}

fn blocks_to_openai_content(blocks: &[ProviderBlock]) -> Value {
    let texts: Vec<String> = blocks
        .iter()
        .filter_map(|block| match block {
            ProviderBlock::Text(text) => Some(text.clone()),
            ProviderBlock::Thinking(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    Value::String(texts.join("\n"))
}

fn user_blocks_to_openai_content(blocks: &[UserBlock]) -> Value {
    let texts: Vec<String> = blocks
        .iter()
        .map(|block| match block {
            UserBlock::Text(text) => text.clone(),
            UserBlock::Media { mime, .. } => format!("[media:{}]", mime),
        })
        .collect();
    Value::String(texts.join("\n"))
}

fn parse_openai_assistant_message(payload: &Value) -> Result<LlmAssistantMessage> {
    let choice = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| anyhow!("missing choices[0] in openai response"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow!("missing choices[0].message in openai response"))?;

    let mut blocks = Vec::new();
    if let Some(content) = message.get("content").and_then(Value::as_str)
        && !content.trim().is_empty()
    {
        blocks.push(ProviderBlock::Text(content.to_string()));
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing tool call id"))?
                .to_string();
            let function = call
                .get("function")
                .ok_or_else(|| anyhow!("missing tool call function"))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing tool call name"))?
                .to_string();
            let arguments_raw = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments_json: Value = serde_json::from_str(arguments_raw).unwrap_or(Value::Null);
            blocks.push(ProviderBlock::ToolCall {
                id,
                name,
                arguments_json,
            });
        }
    }

    let stop_reason = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("tool_calls") => StopReason::ToolCall,
        Some("stop") => StopReason::EndOfTurn,
        Some("length") => StopReason::Error,
        Some("content_filter") => StopReason::Error,
        _ => StopReason::EndOfTurn,
    };

    let message = LlmAssistantMessage {
        content: blocks,
        stop_reason,
        error_message: None,
        usage_tokens: payload_usage_tokens(payload),
    };
    info!(
        target: "borg_llm",
        block_count = message.content.len(),
        stop_reason = ?message.stop_reason,
        "parsed assistant message from provider response"
    );
    Ok(message)
}

fn payload_usage_tokens(payload: &Value) -> Option<u64> {
    payload
        .get("usage")
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{OpenAiApiMode, OpenAiProvider, to_openai_messages};
    use crate::{
        AuthProvider, LlmRequest, Provider, ProviderBlock, ProviderMessage, TranscriptionRequest,
    };

    #[test]
    fn assistant_tool_call_serializes_to_tool_calls_field() {
        let messages = vec![
            ProviderMessage::Assistant {
                content: vec![ProviderBlock::ToolCall {
                    id: "call_123".to_string(),
                    name: "search".to_string(),
                    arguments_json: json!({"query":"users"}),
                }],
            },
            ProviderMessage::ToolResult {
                tool_call_id: "call_123".to_string(),
                name: "search".to_string(),
                content: vec![ProviderBlock::Text("ok".to_string())],
            },
        ];

        let encoded = to_openai_messages(&messages);
        assert_eq!(
            encoded[0].get("role").and_then(|v| v.as_str()),
            Some("assistant")
        );
        assert!(encoded[0].get("tool_calls").is_some());
        assert_eq!(
            encoded[1].get("role").and_then(|v| v.as_str()),
            Some("tool")
        );
    }

    #[test]
    fn builder_requires_api_key() {
        let error = match OpenAiProvider::build().build() {
            Ok(_) => panic!("builder should fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("OpenAI api_key is required"));
    }

    #[test]
    fn builder_sets_default_models() {
        let provider = OpenAiProvider::build()
            .api_key("sk-test")
            .api_mode(OpenAiApiMode::Completions)
            .chat_completions_model("gpt-5.3-codex")
            .audio_transcriptions_model("gpt-4o-mini-transcribe")
            .build()
            .expect("provider");

        let request = LlmRequest {
            model: "".to_string(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            api_key: None,
        };
        let resolved = provider.resolve_chat_model(&request).expect("chat model");
        assert_eq!(resolved, "gpt-5.3-codex");

        let transcribe_request = TranscriptionRequest {
            audio: vec![],
            mime_type: "audio/ogg".to_string(),
            model: None,
            language: None,
            prompt: None,
        };
        let model = transcribe_request
            .model
            .as_ref()
            .and_then(|model| {
                let model = model.trim();
                if model.is_empty() {
                    None
                } else {
                    Some(model.to_string())
                }
            })
            .or_else(|| provider.default_audio_transcriptions_model.clone())
            .expect("transcription model");
        assert_eq!(model, "gpt-4o-mini-transcribe");
    }

    #[test]
    fn provider_name_is_openai() {
        let provider = OpenAiProvider::new("sk-test");
        assert_eq!(Provider::provider_name(&provider), "openai");
    }

    #[test]
    fn auth_provider_exposes_device_code_config() {
        let provider = OpenAiProvider::build()
            .api_key("sk-test")
            .device_code_url("https://example.com/device")
            .device_code_scope("openid profile")
            .build()
            .expect("provider");
        let config = provider
            .device_code_auth_config()
            .expect("device code config");
        assert_eq!(config.url, "https://example.com/device");
        assert_eq!(config.scope.as_deref(), Some("openid profile"));
    }

    #[tokio::test]
    async fn transcribe_requires_explicit_model_configuration() {
        let provider = OpenAiProvider::new("sk-test");
        let request = TranscriptionRequest {
            audio: vec![0x00, 0x01],
            mime_type: "audio/ogg".to_string(),
            model: None,
            language: None,
            prompt: None,
        };

        let error = provider
            .transcribe(&request)
            .await
            .expect_err("missing model");
        assert!(
            error
                .to_string()
                .contains("audio transcription model is required")
        );
    }
}
