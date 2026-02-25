use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::{Value, json};
use tracing::{debug, info};
use tracing::{trace, warn};

use crate::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    UserBlock,
};

const DEFAULT_KALOSM_MODEL: &str = "qwen2.5:0.5b";
const TOOL_RESULT_KIND_TEXT: &str = "text";
const TOOL_RESULT_KIND_THINKING: &str = "thinking";
const TOOL_RESULT_KIND_TOOL_CALL: &str = "tool_call";

#[derive(Clone)]
pub struct KalosmProvider {
    model_spec: String,
    model: std::sync::Arc<tokio::sync::OnceCell<kalosm::language::Llama>>,
}

impl KalosmProvider {
    pub fn new(model_spec: impl Into<String>) -> Self {
        let model_spec = model_spec.into();
        Self {
            model_spec: if model_spec.trim().is_empty() {
                DEFAULT_KALOSM_MODEL.to_string()
            } else {
                model_spec
            },
            model: std::sync::Arc::new(tokio::sync::OnceCell::new()),
        }
    }
}

#[async_trait]
impl Provider for KalosmProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        info!(
            target: "borg_llm",
            model = req.model.as_str(),
            kalosm_model = self.model_spec.as_str(),
            message_count = req.messages.len(),
            tool_count = req.tools.len(),
            "sending local kalosm chat completion request"
        );
        if req.api_key.is_some() {
            debug!(
                target: "borg_llm",
                "kalosm provider ignores request api_key override"
            );
        }

        use kalosm::language::ChatModelExt;

        let model = self
            .model
            .get_or_try_init(|| async { load_model(&self.model_spec).await })
            .await?;
        let prompt = build_kalosm_prompt(req)?;

        trace!(
            target: "borg_llm",
            prompt_len = prompt.len(),
            "constructed kalosm prompt payload"
        );

        let mut chat = model
            .chat()
            .with_system_prompt("You are borg local provider. Return only valid JSON.");
        let raw = chat(&prompt).await?;

        debug!(
            target: "borg_llm",
            response_len = raw.len(),
            "kalosm chat completion succeeded"
        );
        parse_kalosm_response(&raw)
    }
}

async fn load_model(model_spec: &str) -> Result<kalosm::language::Llama> {
    use std::path::PathBuf;

    use kalosm::language::{FileSource, Llama, LlamaSource};

    info!(
        target: "borg_llm",
        kalosm_model = model_spec,
        "loading kalosm model"
    );

    let source = match model_spec {
        "tinyllama" | "tiny-llama" | "tiny_llama_1_1b_chat" => LlamaSource::tiny_llama_1_1b_chat(),
        "qwen2.5:0.5b" | "qwen_2_5_0_5b_instruct" => LlamaSource::qwen_2_5_0_5b_instruct(),
        "qwen2.5:1.5b" | "qwen_2_5_1_5b_instruct" => LlamaSource::qwen_2_5_1_5b_instruct(),
        "phi3.5-mini" | "phi_3_5_mini_4k_instruct" => LlamaSource::phi_3_5_mini_4k_instruct(),
        local if local.starts_with("local:") => {
            let path = PathBuf::from(local.trim_start_matches("local:"));
            if !path.exists() {
                return Err(anyhow!(
                    "local kalosm model path does not exist: {}",
                    path.display()
                ));
            }
            LlamaSource::new(FileSource::local(path))
        }
        other => {
            warn!(
                target: "borg_llm",
                requested_model = other,
                default_model = DEFAULT_KALOSM_MODEL,
                "unknown kalosm model spec, falling back to default"
            );
            LlamaSource::qwen_2_5_0_5b_instruct()
        }
    };

    let model = Llama::builder().with_source(source).build().await?;
    info!(
        target: "borg_llm",
        kalosm_model = model_spec,
        "kalosm model ready"
    );
    Ok(model)
}

fn build_kalosm_prompt(req: &LlmRequest) -> Result<String> {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(provider_message_to_json)
        .collect::<Result<Vec<_>>>()?;

    let tools: Vec<Value> = req
        .tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            })
        })
        .collect();

    let payload = json!({
        "request": {
            "messages": messages,
            "tools": tools,
        },
        "response_format": {
            "type": "json_object",
            "schema": {
                "stop_reason": "end_of_turn | tool_call | error | aborted",
                "assistant_text": "string",
                "tool_calls": [{
                    "id": "string",
                    "name": "string",
                    "arguments": { "any": "json object" }
                }],
                "error_message": "string|null"
            }
        },
        "rules": [
            "Always return strict JSON and no markdown.",
            "When tools are needed, set stop_reason=tool_call and include tool_calls array.",
            "Tool call arguments must be JSON objects and never strings containing JSON.",
            "When no tool call is needed, set stop_reason=end_of_turn and put final answer in assistant_text."
        ],
        "example_tool_call_response": {
            "stop_reason": "tool_call",
            "assistant_text": "",
            "tool_calls": [
                { "id": "call_1", "name": "catalog_lookup", "arguments": { "query": "alpha" } }
            ],
            "error_message": null
        },
        "example_final_response": {
            "stop_reason": "end_of_turn",
            "assistant_text": "Resolved using tool output.",
            "tool_calls": [],
            "error_message": null
        }
    });

    Ok(serde_json::to_string_pretty(&payload)?)
}

fn provider_message_to_json(message: &ProviderMessage) -> Result<Value> {
    match message {
        ProviderMessage::System { text } => Ok(json!({
            "type": "system",
            "text": text,
        })),
        ProviderMessage::User { content } => {
            let blocks: Vec<Value> = content.iter().map(user_block_to_json).collect();
            Ok(json!({
                "type": "user",
                "content": blocks,
            }))
        }
        ProviderMessage::Assistant { content } => {
            let blocks: Vec<Value> = content.iter().map(provider_block_to_json).collect();
            Ok(json!({
                "type": "assistant",
                "content": blocks,
            }))
        }
        ProviderMessage::ToolResult {
            tool_call_id,
            name,
            content,
        } => {
            let blocks: Vec<Value> = content.iter().map(provider_block_to_json).collect();
            Ok(json!({
                "type": "tool_result",
                "tool_call_id": tool_call_id,
                "name": name,
                "content": blocks,
            }))
        }
    }
}

fn provider_block_to_json(block: &ProviderBlock) -> Value {
    match block {
        ProviderBlock::Text(text) => json!({
            "kind": TOOL_RESULT_KIND_TEXT,
            "text": text,
        }),
        ProviderBlock::Thinking(text) => json!({
            "kind": TOOL_RESULT_KIND_THINKING,
            "text": text,
        }),
        ProviderBlock::ToolCall {
            id,
            name,
            arguments_json,
        } => json!({
            "kind": TOOL_RESULT_KIND_TOOL_CALL,
            "id": id,
            "name": name,
            "arguments": arguments_json,
        }),
    }
}

fn user_block_to_json(block: &UserBlock) -> Value {
    match block {
        UserBlock::Text(text) => json!({
            "kind": "text",
            "text": text,
        }),
        UserBlock::Media { mime, .. } => json!({
            "kind": "media",
            "mime": mime,
            "data": "[binary omitted]",
        }),
    }
}

fn parse_kalosm_response(raw: &str) -> Result<LlmAssistantMessage> {
    let json_candidate = extract_json_object(raw);
    let parsed: Value = match serde_json::from_str(&json_candidate) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                target: "borg_llm",
                error = %err,
                "kalosm response was not valid json, falling back to plain text"
            );
            return Ok(LlmAssistantMessage {
                content: vec![ProviderBlock::Text(raw.trim().to_string())],
                stop_reason: StopReason::EndOfTurn,
                error_message: None,
            });
        }
    };

    trace!(
        target: "borg_llm",
        response = ?parsed,
        "parsed kalosm json response"
    );

    let mut content = Vec::new();
    if let Some(text) = parsed.get("assistant_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            content.push(ProviderBlock::Text(text.to_string()));
        }
    }

    let mut tool_call_count = 0usize;
    if let Some(tool_calls) = parsed.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("call_local")
                .to_string();
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("kalosm tool_call missing name"))?
                .to_string();
            let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let arguments_json = if arguments.is_object() {
                arguments
            } else {
                warn!(
                    target: "borg_llm",
                    tool_name = name.as_str(),
                    "kalosm emitted non-object tool arguments; coercing to empty object"
                );
                json!({})
            };
            content.push(ProviderBlock::ToolCall {
                id,
                name,
                arguments_json,
            });
            tool_call_count += 1;
        }
    }

    let stop_reason = parsed
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(parse_stop_reason)
        .unwrap_or_else(|| {
            if tool_call_count > 0 {
                StopReason::ToolCall
            } else {
                StopReason::EndOfTurn
            }
        });

    let message = LlmAssistantMessage {
        content,
        stop_reason,
        error_message: parsed
            .get("error_message")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    };
    info!(
        target: "borg_llm",
        block_count = message.content.len(),
        stop_reason = ?message.stop_reason,
        "parsed assistant message from kalosm response"
    );
    Ok(message)
}

fn parse_stop_reason(raw: &str) -> StopReason {
    match raw {
        "tool_call" | "tool_calls" => StopReason::ToolCall,
        "error" => StopReason::Error,
        "aborted" => StopReason::Aborted,
        _ => StopReason::EndOfTurn,
    }
}

fn extract_json_object(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return trimmed.to_string();
    }

    let fenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"));
    if let Some(value) = fenced {
        let v = value.trim();
        if v.starts_with('{') && v.ends_with('}') {
            return v.to_string();
        }
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            return trimmed[start..=end].to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_call_json_response() {
        let raw = r#"{
          "stop_reason":"tool_call",
          "assistant_text":"",
          "tool_calls":[{"id":"call_1","name":"search","arguments":{"query":"abc"}}],
          "error_message":null
        }"#;

        let parsed = parse_kalosm_response(raw).unwrap();
        assert!(matches!(parsed.stop_reason, StopReason::ToolCall));
        assert_eq!(parsed.content.len(), 1);
        assert!(matches!(
            &parsed.content[0],
            ProviderBlock::ToolCall { name, .. } if name == "search"
        ));
    }

    #[test]
    fn falls_back_to_plain_text_on_invalid_json() {
        let parsed = parse_kalosm_response("not json").unwrap();
        assert!(matches!(parsed.stop_reason, StopReason::EndOfTurn));
        assert!(matches!(
            &parsed.content[0],
            ProviderBlock::Text(text) if text == "not json"
        ));
    }
}
