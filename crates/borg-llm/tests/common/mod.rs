use borg_llm::completion::{CompletionEvent, OutputContent, OutputItem};
use borg_llm::error::{Error, LlmResult};
use borg_llm::tools::{RawToolDefinition, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EchoResponse {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum TestTools {
    Ping { value: String },
}

impl TypedTool for TestTools {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        vec![RawToolDefinition::function(
            "ping",
            Some("Echo a value back to the caller"),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"]
            }),
        )]
    }

    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
        match name {
            "ping" => Ok(TestTools::Ping {
                value: serde_json::from_value::<PingArgs>(arguments)
                    .map_err(|e| Error::parse("tool arguments", e))?
                    .value,
            }),
            other => Err(Error::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingArgs {
    pub value: String,
}

pub async fn assert_non_empty_text_stream<C>(
    stream: &mut borg_llm::completion::CompletionEventStream<C, String>,
) -> LlmResult<()> {
    let mut saw_text_delta = false;
    let mut final_content = None;

    while let Some(event) = stream.recv().await {
        match event? {
            CompletionEvent::TextDelta { text } => {
                if !text.trim().is_empty() {
                    saw_text_delta = true;
                }
            }
            CompletionEvent::ReasoningDelta { .. } => {}
            CompletionEvent::ToolCall { .. } => {}
            CompletionEvent::Done(response) => {
                final_content = response.output.iter().find_map(|item| match item {
                    OutputItem::Message { content, .. } => {
                        content.iter().find_map(|content| match content {
                            OutputContent::Text { text } => Some(text.clone()),
                            OutputContent::Structured { .. } => None,
                        })
                    }
                    OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
                });
                break;
            }
        }
    }

    assert!(saw_text_delta, "expected at least one non-empty text delta");
    assert!(
        final_content
            .as_deref()
            .is_some_and(|content| !content.trim().is_empty()),
        "expected non-empty final streamed response, got {:?}",
        final_content
    );

    Ok(())
}

pub async fn assert_streamed_typed_response(
    stream: &mut borg_llm::completion::CompletionEventStream<(), EchoResponse>,
) -> LlmResult<()> {
    let mut saw_text_delta = false;
    let mut final_response = None;

    while let Some(event) = stream.recv().await {
        match event? {
            CompletionEvent::TextDelta { text } => {
                if !text.trim().is_empty() {
                    saw_text_delta = true;
                }
            }
            CompletionEvent::ReasoningDelta { .. } => {}
            CompletionEvent::ToolCall { .. } => {}
            CompletionEvent::Done(response) => {
                final_response = response.output.iter().find_map(|item| match item {
                    OutputItem::Message { content, .. } => {
                        content.iter().find_map(|content| match content {
                            OutputContent::Structured { value } => Some(value.clone()),
                            OutputContent::Text { .. } => None,
                        })
                    }
                    OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
                });
                break;
            }
        }
    }

    let final_response = final_response.expect("expected final streamed typed response");
    assert!(
        saw_text_delta,
        "expected at least one non-empty text delta before typed done event"
    );
    assert!(
        !final_response.value.trim().is_empty(),
        "expected non-empty typed streamed response value, got {:?}",
        final_response
    );

    Ok(())
}

pub async fn assert_streamed_ping_tool_call(
    stream: &mut borg_llm::completion::CompletionEventStream<TestTools, String>,
) -> LlmResult<()> {
    let mut saw_ping_event = false;
    let mut final_tool_calls = Vec::new();

    while let Some(event) = stream.recv().await {
        match event? {
            CompletionEvent::TextDelta { .. } => {}
            CompletionEvent::ReasoningDelta { .. } => {}
            CompletionEvent::ToolCall { call } => {
                if matches!(call.tool, TestTools::Ping { ref value } if value == "hello-tool") {
                    saw_ping_event = true;
                }
            }
            CompletionEvent::Done(response) => {
                final_tool_calls = response
                    .output
                    .into_iter()
                    .filter_map(|item| match item {
                        OutputItem::ToolCall { call } => Some(call),
                        OutputItem::Message { .. } | OutputItem::Reasoning { .. } => None,
                    })
                    .collect();
                break;
            }
        }
    }

    let saw_ping_in_done = final_tool_calls
        .iter()
        .any(|call| matches!(call.tool, TestTools::Ping { ref value } if value == "hello-tool"));

    assert!(
        saw_ping_event || saw_ping_in_done,
        "expected streamed ping tool call event or final decoded tool call, got {:?}",
        final_tool_calls
    );

    Ok(())
}
