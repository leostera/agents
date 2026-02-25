use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

use crate::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    ToolDescriptor,
};

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Clone)]
pub struct OpenAiProvider {
    http: Client,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        let body = json!({
            "model": req.model,
            "messages": to_openai_messages(&req.messages),
            "tools": to_openai_tools(&req.tools),
            "tool_choice": "auto",
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        });

        let api_key = req.api_key.as_deref().unwrap_or(&self.api_key);
        let response = self
            .http
            .post(OPENAI_CHAT_COMPLETIONS_URL)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!("openai chat completions returned {}", response.status()));
        }

        let payload: Value = response.json().await?;
        parse_openai_assistant_message(&payload)
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
                "content": blocks_to_openai_content(content)
            }),
            ProviderMessage::Assistant { content } => json!({
                "role": "assistant",
                "content": blocks_to_openai_content(content)
            }),
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

    Ok(LlmAssistantMessage {
        content: blocks,
        stop_reason,
        error_message: None,
    })
}
