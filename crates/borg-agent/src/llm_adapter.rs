use std::collections::HashMap;

use anyhow::{Result, anyhow};
use borg_llm::{ProviderBlock, ProviderMessage, UserBlock};
use serde::Serialize;

use crate::{Message, ToolResultData};

const INTERRUPTED_TOOL_RESULT_MESSAGE: &str =
    "tool error: tool execution interrupted before a result was recorded";

pub fn to_provider_messages<TToolCall, TToolResult>(
    messages: &[Message<TToolCall, TToolResult>],
) -> Result<Vec<ProviderMessage>>
where
    TToolCall: Serialize,
    TToolResult: Serialize,
{
    let mut pending_tool_calls: HashMap<String, String> = HashMap::new();
    let mut immediate_tool_call: Option<(String, String)> = None;
    let mut provider_messages = Vec::new();

    for message in messages {
        match message {
            Message::System { content } => {
                flush_interrupted_tool_call(
                    &mut provider_messages,
                    &mut pending_tool_calls,
                    &mut immediate_tool_call,
                );
                provider_messages.push(ProviderMessage::System {
                    text: content.clone(),
                });
            }
            Message::User { content } => {
                flush_interrupted_tool_call(
                    &mut provider_messages,
                    &mut pending_tool_calls,
                    &mut immediate_tool_call,
                );
                provider_messages.push(ProviderMessage::User {
                    content: vec![UserBlock::Text(content.clone())],
                });
            }
            Message::UserAudio { transcript, .. } => {
                flush_interrupted_tool_call(
                    &mut provider_messages,
                    &mut pending_tool_calls,
                    &mut immediate_tool_call,
                );
                provider_messages.push(ProviderMessage::User {
                    content: vec![UserBlock::Text(transcript.clone())],
                });
            }
            Message::Assistant { content } => {
                flush_interrupted_tool_call(
                    &mut provider_messages,
                    &mut pending_tool_calls,
                    &mut immediate_tool_call,
                );
                provider_messages.push(ProviderMessage::Assistant {
                    content: vec![ProviderBlock::Text(content.clone())],
                });
            }
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => {
                flush_interrupted_tool_call(
                    &mut provider_messages,
                    &mut pending_tool_calls,
                    &mut immediate_tool_call,
                );
                pending_tool_calls.insert(tool_call_id.clone(), name.clone());
                immediate_tool_call = Some((tool_call_id.clone(), name.clone()));
                provider_messages.push(ProviderMessage::Assistant {
                    content: vec![ProviderBlock::ToolCall {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        arguments_json: serde_json::to_value(arguments)?,
                    }],
                });
            }
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => {
                if let Some((expected_id, _expected_name)) = immediate_tool_call.as_ref()
                    && expected_id != tool_call_id
                {
                    flush_interrupted_tool_call(
                        &mut provider_messages,
                        &mut pending_tool_calls,
                        &mut immediate_tool_call,
                    );
                }
                if !pending_tool_calls.contains_key(tool_call_id) {
                    return Err(anyhow!(
                        "orphan tool result detected for tool_call_id={} tool_name={}; missing preceding tool call in context",
                        tool_call_id,
                        name
                    ));
                }
                pending_tool_calls.remove(tool_call_id);
                immediate_tool_call = None;
                provider_messages.push(ProviderMessage::ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    content: vec![ProviderBlock::Text(tool_result_to_text(content))],
                });
            }
            Message::SessionEvent { .. } => {}
        }
    }

    flush_interrupted_tool_call(
        &mut provider_messages,
        &mut pending_tool_calls,
        &mut immediate_tool_call,
    );

    Ok(provider_messages)
}

pub fn tool_result_to_text<TToolResult>(content: &ToolResultData<TToolResult>) -> String
where
    TToolResult: Serialize,
{
    match content {
        ToolResultData::Text(text) => text.clone(),
        ToolResultData::Capabilities(items) => format!("capabilities: {}", items.len()),
        ToolResultData::Execution { result, duration } => {
            let serialized =
                serde_json::to_string(result).unwrap_or_else(|_| "<invalid_result>".to_string());
            format!("execution result in {}ms: {}", duration.as_millis(), serialized)
        }
        ToolResultData::Error { message } => format!("tool error: {}", message),
    }
}

fn flush_interrupted_tool_call(
    provider_messages: &mut Vec<ProviderMessage>,
    pending_tool_calls: &mut HashMap<String, String>,
    immediate_tool_call: &mut Option<(String, String)>,
) {
    let Some((tool_call_id, name)) = immediate_tool_call.take() else {
        return;
    };

    if pending_tool_calls.remove(&tool_call_id).is_none() {
        return;
    }

    provider_messages.push(ProviderMessage::ToolResult {
        tool_call_id,
        name,
        content: vec![ProviderBlock::Text(
            INTERRUPTED_TOOL_RESULT_MESSAGE.to_string(),
        )],
    });
}
