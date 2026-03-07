use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info, trace};

use crate::providers::call_trace::ProviderCallTrace;
use crate::{
    LlmAssistantMessage, LlmError, LlmRequest, Provider, ProviderBlock, ProviderMessage, Result,
    StopReason, ToolDescriptor, TranscriptionRequest, UserBlock,
};

const OPENROUTER_CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const OPENROUTER_PROVIDER_NAME: &str = "openrouter";

#[derive(Clone)]
pub struct OpenRouterProvider {
    http: Client,
    api_key: String,
    chat_completions_url: String,
    models_url: String,
    default_chat_completions_model: Option<String>,
    default_audio_transcriptions_model: Option<String>,
}

impl OpenRouterProvider {
    pub fn build() -> OpenRouterProviderBuilder {
        OpenRouterProviderBuilder::new()
    }

    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            chat_completions_url: OPENROUTER_CHAT_COMPLETIONS_URL.to_string(),
            models_url: OPENROUTER_MODELS_URL.to_string(),
            default_chat_completions_model: None,
            default_audio_transcriptions_model: None,
        }
    }

    pub fn new_with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            chat_completions_url: format!("{}/v1/chat/completions", base),
            models_url: format!("{}/v1/models", base),
            default_chat_completions_model: None,
            default_audio_transcriptions_model: None,
        }
    }

    fn resolve_chat_model(&self, req: &LlmRequest) -> Result<String> {
        if !req.model.trim().is_empty() {
            return Ok(req.model.clone());
        }
        if let Some(default_model) = &self.default_chat_completions_model {
            return Ok(default_model.clone());
        }
        Err(LlmError::configuration(
            "chat completion model is required (set request.model or configure chat_completions_model on OpenRouterProvider::build())",
        ))
    }

    fn resolve_transcription_model(&self, req: &TranscriptionRequest) -> Result<String> {
        req.model
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
                LlmError::configuration(
                    "audio transcription model is required (set request.model or configure audio_transcriptions_model on OpenRouterProvider::build())"
                )
            })
    }
}

pub struct OpenRouterProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    chat_completions_model: Option<String>,
    audio_transcriptions_model: Option<String>,
}

impl OpenRouterProviderBuilder {
    pub fn new() -> Self {
        Self {
            api_key: None,
            base_url: None,
            chat_completions_model: None,
            audio_transcriptions_model: None,
        }
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
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

    pub fn audio_transcriptions_model(mut self, model: impl Into<String>) -> Self {
        self.audio_transcriptions_model = Some(model.into());
        self
    }

    pub fn build(self) -> Result<OpenRouterProvider> {
        let api_key = self
            .api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .ok_or_else(|| LlmError::configuration("OpenRouter api_key is required"))?;
        let mut provider = if let Some(base_url) = self.base_url {
            OpenRouterProvider::new_with_base_url(api_key, base_url)
        } else {
            OpenRouterProvider::new(api_key)
        };
        provider.default_chat_completions_model = normalize_optional(self.chat_completions_model);
        provider.default_audio_transcriptions_model =
            normalize_optional(self.audio_transcriptions_model);
        Ok(provider)
    }
}

impl Default for OpenRouterProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    fn provider_name(&self) -> &'static str {
        OPENROUTER_PROVIDER_NAME
    }

    fn supports_chat_completion(&self) -> bool {
        true
    }

    fn supports_audio_transcription(&self) -> bool {
        true
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let provider = self.provider_name();
        let model = self.resolve_chat_model(req)?;
        info!(
            target: "borg_llm",
            model = model.as_str(),
            message_count = req.messages.len(),
            tool_count = req.tools.len(),
            "sending openrouter chat completion request"
        );
        let body = json!({
            "model": model,
            "messages": to_openai_messages(&req.messages),
            "tools": to_openai_tools(&req.tools),
            "tool_choice": "auto",
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "reasoning": req.reasoning_effort.map(|effort| {
                json!({
                    "effort": openrouter_reasoning_effort_label(effort)
                })
            }),
        });
        let call =
            ProviderCallTrace::sent(provider, "chat_completion", model.clone(), body.clone());
        let api_key = req.api_key.as_deref().unwrap_or(&self.api_key);
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
            let error_message = format!("openrouter chat completions returned {}", status);
            call.failed(Some(status), None, Some(body.as_str()), &error_message)
                .await;
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "openrouter chat completion request failed"
            );
            let reason = provider_error_reason_from_body(&body)
                .unwrap_or_else(|| format!("request failed with status {}", status));
            return Err(LlmError::ProviderResponse {
                provider,
                capability: "chat_completion",
                reason: format!("status {}: {}", status, reason),
            });
        }

        debug!(
            target: "borg_llm",
            status = %status,
            "openrouter chat completion request succeeded"
        );
        let payload: Value = response.json().await?;
        call.succeeded(status, &payload).await;
        trace!(target: "borg_llm", payload = ?payload, "raw openrouter chat payload");
        parse_assistant_message(&payload)
    }

    async fn available_models(&self) -> Result<Vec<String>> {
        let provider = self.provider_name();
        let response = self
            .http
            .get(&self.models_url)
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(LlmError::ProviderHttp {
                provider,
                capability: "available_models",
                status: status.as_u16(),
            });
        }
        let payload: Value = response.json().await?;
        let mut models = payload
            .get("data")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str))
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        models.sort();
        models.dedup();
        Ok(models)
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        let provider = self.provider_name();
        let model = self.resolve_transcription_model(req)?;
        let format = mime_to_input_audio_format(&req.mime_type)?;
        let audio_base64 = STANDARD.encode(&req.audio);
        let prompt = req
            .prompt
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Transcribe this audio. Return only the transcript text.");

        info!(
            target: "borg_llm",
            model = model.as_str(),
            mime_type = req.mime_type.as_str(),
            bytes = req.audio.len(),
            "sending openrouter audio transcription request"
        );

        let body = json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {"type": "input_audio", "input_audio": {"data": audio_base64, "format": format}}
                    ]
                }
            ]
        });
        let call =
            ProviderCallTrace::sent(provider, "audio_transcription", model.clone(), body.clone());

        let response = self
            .http
            .post(&self.chat_completions_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let error_message = format!("openrouter transcriptions returned {}", status);
            call.failed(Some(status), None, Some(body.as_str()), &error_message)
                .await;
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "openrouter transcription request failed"
            );
            let reason = provider_error_reason_from_body(&body)
                .unwrap_or_else(|| format!("request failed with status {}", status));
            return Err(LlmError::ProviderResponse {
                provider,
                capability: "audio_transcription",
                reason: format!("status {}: {}", status, reason),
            });
        }

        let payload: Value = response.json().await?;
        call.succeeded(status, &payload).await;
        extract_text_from_chat_payload(&payload)
            .ok_or_else(|| LlmError::message("missing text in openrouter transcription response"))
    }
}

fn mime_to_input_audio_format(mime: &str) -> Result<&'static str> {
    match mime {
        "audio/wav" => Ok("wav"),
        "audio/mp3" | "audio/mpeg" => Ok("mp3"),
        // OpenRouter docs list wav/mp3. We still allow common telegram mime types by mapping to mp3.
        "audio/ogg" | "audio/opus" | "audio/webm" => Ok("mp3"),
        _ => Err(LlmError::configuration(format!(
            "unsupported audio mime type `{}`",
            mime
        ))),
    }
}

fn provider_error_reason_from_body(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(payload) = serde_json::from_str::<Value>(trimmed) {
        if let Some(reason) = payload
            .get("error")
            .and_then(|error| error.get("metadata"))
            .and_then(|metadata| metadata.get("raw"))
            .and_then(Value::as_str)
            .and_then(extract_nested_error_message)
        {
            return Some(reason);
        }

        if let Some(reason) = payload
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(reason.to_string());
        }

        if let Some(reason) = payload
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(reason.to_string());
        }
    }

    Some(trimmed.to_string())
}

fn extract_nested_error_message(raw: &str) -> Option<String> {
    let nested: Value = serde_json::from_str(raw).ok()?;
    nested
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn extract_text_from_chat_payload(payload: &Value) -> Option<String> {
    let choice = payload.get("choices")?.as_array()?.first()?;
    let message = choice.get("message")?;
    if let Some(content) = message.get("content").and_then(Value::as_str)
        && !content.trim().is_empty()
    {
        return Some(content.trim().to_string());
    }
    if let Some(content_blocks) = message.get("content").and_then(Value::as_array) {
        let text = content_blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
}

fn openrouter_reasoning_effort_label(effort: crate::ReasoningEffort) -> &'static str {
    match effort {
        crate::ReasoningEffort::Minimal => "low",
        crate::ReasoningEffort::Low => "low",
        crate::ReasoningEffort::Medium => "medium",
        crate::ReasoningEffort::High => "high",
        crate::ReasoningEffort::XHigh => "high",
    }
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
                    "parameters": normalize_tool_schema(&tool.input_schema)
                }
            })
        })
        .collect()
}

fn normalize_tool_schema(schema: &Value) -> Value {
    let mut normalized = schema.clone();
    let Some(root) = normalized.as_object_mut() else {
        return json!({
            "type": "object",
            "properties": {}
        });
    };

    if !root.contains_key("type") {
        root.insert("type".to_string(), Value::String("object".to_string()));
    }

    let is_object_schema = root
        .get("type")
        .and_then(Value::as_str)
        .is_none_or(|kind| kind == "object");
    if is_object_schema && !root.contains_key("properties") {
        root.insert("properties".to_string(), json!({}));
    }

    normalized
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

fn parse_assistant_message(payload: &Value) -> Result<LlmAssistantMessage> {
    let choice = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| LlmError::message("missing choices[0] in openrouter response"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| LlmError::message("missing choices[0].message in openrouter response"))?;

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
                .ok_or_else(|| LlmError::message("missing tool call id"))?
                .to_string();
            let function = call
                .get("function")
                .ok_or_else(|| LlmError::message("missing tool call function"))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| LlmError::message("missing tool call name"))?
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

    let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
    let stop_reason = match finish_reason {
        Some("tool_calls") => StopReason::ToolCall,
        Some("stop") => StopReason::EndOfTurn,
        Some("length") => StopReason::Error,
        Some("content_filter") => StopReason::Error,
        _ => StopReason::EndOfTurn,
    };
    let error_message = match finish_reason {
        Some("length") => Some("openrouter chat completion stopped due to token limit".to_string()),
        Some("content_filter") => {
            Some("openrouter chat completion blocked by content filter".to_string())
        }
        _ => None,
    };

    assistant_message_or_error(LlmAssistantMessage {
        content: blocks,
        stop_reason,
        error_message,
        usage_tokens: payload_usage_tokens(payload),
    })
}

fn assistant_message_or_error(message: LlmAssistantMessage) -> Result<LlmAssistantMessage> {
    let explicit_error = message
        .error_message
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted)
        || explicit_error.is_some()
    {
        return Err(LlmError::ProviderResponse {
            provider: OPENROUTER_PROVIDER_NAME,
            capability: "chat_completion",
            reason: explicit_error
                .unwrap_or_else(|| "assistant returned error stop reason".to_string()),
        });
    }
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
    use super::{OpenRouterProvider, mime_to_input_audio_format, provider_error_reason_from_body};
    use crate::{LlmRequest, Provider, TranscriptionRequest};

    #[test]
    fn builder_requires_api_key() {
        let error = match OpenRouterProvider::build().build() {
            Ok(_) => panic!("builder should fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("OpenRouter api_key is required"));
    }

    #[test]
    fn provider_name_is_openrouter() {
        let provider = OpenRouterProvider::new("or-key");
        assert_eq!(Provider::provider_name(&provider), "openrouter");
    }

    #[test]
    fn builder_sets_default_models() {
        let provider = OpenRouterProvider::build()
            .api_key("or-key")
            .chat_completions_model("moonshot/kimi-k2")
            .audio_transcriptions_model("openai/gpt-4o-mini-transcribe")
            .build()
            .expect("provider");
        let request = LlmRequest {
            model: "".to_string(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
            api_key: None,
        };
        let model = provider.resolve_chat_model(&request).expect("chat model");
        assert_eq!(model, "moonshot/kimi-k2");
    }

    #[tokio::test]
    async fn transcribe_requires_explicit_model_configuration() {
        let provider = OpenRouterProvider::new("or-key");
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

    #[test]
    fn mime_mappings_cover_telegram_inputs() {
        assert_eq!(mime_to_input_audio_format("audio/ogg").expect("ogg"), "mp3");
        assert_eq!(
            mime_to_input_audio_format("audio/opus").expect("opus"),
            "mp3"
        );
        assert_eq!(
            mime_to_input_audio_format("audio/mpeg").expect("mpeg"),
            "mp3"
        );
        assert!(mime_to_input_audio_format("application/octet-stream").is_err());
    }

    #[test]
    fn provider_error_reason_prefers_nested_error_message() {
        let body = r#"{"error":{"message":"maximum context length exceeded"}}"#;
        let reason = provider_error_reason_from_body(body).expect("reason");
        assert_eq!(reason, "maximum context length exceeded");
    }

    #[test]
    fn provider_error_reason_uses_metadata_raw_when_present() {
        let body = r#"{
          "error":{
            "message":"Provider returned error",
            "metadata":{
              "raw":"{\"error\":{\"code\":400,\"message\":\"The input token count exceeds the maximum number of tokens allowed 1048576.\",\"status\":\"INVALID_ARGUMENT\"}}"
            }
          }
        }"#;
        let reason = provider_error_reason_from_body(body).expect("reason");
        assert_eq!(
            reason,
            "The input token count exceeds the maximum number of tokens allowed 1048576."
        );
    }
}
