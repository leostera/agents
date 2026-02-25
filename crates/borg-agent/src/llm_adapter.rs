use borg_llm::{ProviderBlock, ProviderMessage, UserBlock};

use crate::{Message, ToolResultData};

pub fn to_provider_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::System { content } => Some(ProviderMessage::System {
                text: content.clone(),
            }),
            Message::User { content } => Some(ProviderMessage::User {
                content: vec![UserBlock::Text(content.clone())],
            }),
            Message::Assistant { content } => Some(ProviderMessage::Assistant {
                content: vec![ProviderBlock::Text(content.clone())],
            }),
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => Some(ProviderMessage::Assistant {
                content: vec![ProviderBlock::ToolCall {
                    id: tool_call_id.clone(),
                    name: name.clone(),
                    arguments_json: arguments.clone(),
                }],
            }),
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => Some(ProviderMessage::ToolResult {
                tool_call_id: tool_call_id.clone(),
                name: name.clone(),
                content: vec![ProviderBlock::Text(tool_result_to_text(content))],
            }),
            Message::SessionEvent { .. } => None,
        })
        .collect()
}

pub fn tool_result_to_text(content: &ToolResultData) -> String {
    match content {
        ToolResultData::Text(text) => text.clone(),
        ToolResultData::Capabilities(items) => format!("capabilities: {}", items.len()),
        ToolResultData::Execution {
            result,
            duration_ms,
        } => format!("execution result in {}ms: {}", duration_ms, result),
        ToolResultData::Error { message } => format!("tool error: {}", message),
    }
}
