use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info, trace};

use crate::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    ToolDescriptor, TranscriptionRequest, UserBlock,
};

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_AUDIO_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const DEFAULT_TRANSCRIPTION_MODEL: &str = "gpt-4o-mini-transcribe";

#[derive(Clone)]
pub struct OpenAiProvider {
    http: Client,
    api_key: String,
    chat_completions_url: String,
    audio_transcriptions_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            chat_completions_url: OPENAI_CHAT_COMPLETIONS_URL.to_string(),
            audio_transcriptions_url: OPENAI_AUDIO_TRANSCRIPTIONS_URL.to_string(),
        }
    }

    pub fn new_with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            chat_completions_url: format!("{}/v1/chat/completions", base),
            audio_transcriptions_url: format!("{}/v1/audio/transcriptions", base),
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        info!(
            target: "borg_llm",
            model = req.model.as_str(),
            message_count = req.messages.len(),
            tool_count = req.tools.len(),
            "sending chat completion request"
        );
        let body = json!({
            "model": req.model,
            "messages": to_openai_messages(&req.messages),
            "tools": to_openai_tools(&req.tools),
            "tool_choice": "auto",
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });
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
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "chat completion request failed"
            );
            return Err(anyhow!("openai chat completions returned {}", status));
        }

        debug!(target: "borg_llm", status = %status, "chat completion request succeeded");
        let payload: Value = response.json().await?;
        trace!(target: "borg_llm", payload = ?payload, "raw chat completion payload");
        parse_openai_assistant_message(&payload)
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        let model = req
            .model
            .as_deref()
            .unwrap_or(DEFAULT_TRANSCRIPTION_MODEL)
            .to_string();
        info!(
            target: "borg_llm",
            model = model.as_str(),
            mime_type = req.mime_type.as_str(),
            bytes = req.audio.len(),
            "sending audio transcription request"
        );

        let file_name = transcription_filename_for_mime(&req.mime_type);
        let part = reqwest::multipart::Part::bytes(req.audio.clone())
            .file_name(file_name)
            .mime_str(&req.mime_type)?;
        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", model);
        if let Some(language) = req.language.as_ref().filter(|value| !value.trim().is_empty()) {
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
            error!(
                target: "borg_llm",
                status = %status,
                response_body = body.as_str(),
                "audio transcription request failed"
            );
            return Err(anyhow!("openai audio transcriptions returned {}", status));
        }

        let payload: Value = response.json().await?;
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
    if let Some(content) = message.get("content").and_then(Value::as_str) {
        if !content.trim().is_empty() {
            blocks.push(ProviderBlock::Text(content.to_string()));
        }
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
    };
    info!(
        target: "borg_llm",
        block_count = message.content.len(),
        stop_reason = ?message.stop_reason,
        "parsed assistant message from provider response"
    );
    Ok(message)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::to_openai_messages;
    use crate::{ProviderBlock, ProviderMessage};

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
}
