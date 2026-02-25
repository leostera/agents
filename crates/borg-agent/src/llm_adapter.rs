use std::collections::HashSet;

use anyhow::{Result, anyhow};
use borg_llm::{ProviderBlock, ProviderMessage, UserBlock};

use crate::{Message, ToolResultData};

pub fn to_provider_messages(messages: &[Message]) -> Result<Vec<ProviderMessage>> {
    let mut pending_tool_calls: HashSet<String> = HashSet::new();
    let mut immediate_tool_call: Option<String> = None;

    messages
        .iter()
        .filter_map(|message| match message {
            Message::System { content } => {
                immediate_tool_call = None;
                Some(Ok(ProviderMessage::System {
                    text: content.clone(),
                }))
            }
            Message::User { content } => {
                immediate_tool_call = None;
                Some(Ok(ProviderMessage::User {
                    content: vec![UserBlock::Text(content.clone())],
                }))
            }
            Message::Assistant { content } => {
                immediate_tool_call = None;
                Some(Ok(ProviderMessage::Assistant {
                    content: vec![ProviderBlock::Text(content.clone())],
                }))
            }
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => {
                pending_tool_calls.insert(tool_call_id.clone());
                immediate_tool_call = Some(tool_call_id.clone());
                Some(Ok(ProviderMessage::Assistant {
                    content: vec![ProviderBlock::ToolCall {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        arguments_json: arguments.clone(),
                    }],
                }))
            }
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => {
                if immediate_tool_call.as_deref() != Some(tool_call_id.as_str()) {
                    return Some(Err(anyhow!(
                        "invalid tool message ordering for tool_call_id={} tool_name={}; tool result must immediately follow matching tool call",
                        tool_call_id,
                        name
                    )));
                }
                if !pending_tool_calls.remove(tool_call_id) {
                    return Some(Err(anyhow!(
                        "orphan tool result detected for tool_call_id={} tool_name={}; missing preceding tool call in context",
                        tool_call_id,
                        name
                    )));
                }
                immediate_tool_call = None;

                Some(Ok(ProviderMessage::ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    content: vec![ProviderBlock::Text(tool_result_to_text(content))],
                }))
            }
            Message::SessionEvent { .. } => None,
        })
        .collect::<Result<Vec<_>>>()
}

pub fn tool_result_to_text(content: &ToolResultData) -> String {
    match content {
        ToolResultData::Text(text) => text.clone(),
        ToolResultData::Capabilities(items) => format!("capabilities: {}", items.len()),
        ToolResultData::Execution { result, duration } => {
            format!("execution result in {}ms: {}", duration.as_millis(), result)
        }
        ToolResultData::Error { message } => format!("tool error: {}", message),
    }
}
