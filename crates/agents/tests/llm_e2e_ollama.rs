mod common;

use agents::completion::{
    CompletionRequest, InputItem, ModelSelector, OutputContent, OutputItem, RawCompletionRequest,
    RawInputContent, RawInputItem, RawOutputContent, RawOutputItem, ResponseMode, Role,
    Temperature, TokenLimit, ToolChoice, TopK, TopP,
};
use agents::error::LlmResult;
use agents::provider::LlmProvider;
use agents::response::TypedResponse;
use agents::tools::{RawToolDefinition, TypedToolSet};
use agents_test::{TestContext, TestProvider};
use common::{
    EchoResponse, TestTools, assert_streamed_ping_tool_call, assert_streamed_typed_response,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serial_test::serial;

const OLLAMA_TEXT_MODEL: &str = "qwen2.5:7b";
const OLLAMA_STRUCTURED_OUTPUT_MODEL: &str = "qwen2.5:7b";
const OLLAMA_TOOL_MODEL: &str = "qwen2.5:7b";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DefaultedJudgeResponse {
    passed: bool,
    score: f32,
    summary: String,
    evidence: DefaultedJudgeEvidence,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DefaultedJudgeEvidence {
    #[serde(default)]
    strengths: Vec<String>,
    #[serde(default)]
    concerns: Vec<String>,
}

#[tokio::test]
#[serial]
async fn ollama_provider_chat_raw_returns_text_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let provider = ctx.ollama_provider_for_model(OLLAMA_TEXT_MODEL).await?;

    let response = provider
        .chat_raw(RawCompletionRequest {
            model: ModelSelector::from_model(OLLAMA_TEXT_MODEL),
            input: vec![RawInputItem::Message {
                role: Role::User,
                content: vec![RawInputContent::Text {
                    text: "Reply with a short plain-text acknowledgment. Do not return JSON."
                        .to_string(),
                }],
            }],
            temperature: Temperature::Value(0.0),
            top_p: TopP::ProviderDefault,
            top_k: TopK::ProviderDefault,
            token_limit: TokenLimit::Max(32),
            response_mode: ResponseMode::Buffered,
            tools: None,
            tool_choice: ToolChoice::ProviderDefault,
            response_format: None,
        })
        .await?;

    let text = response.output.iter().find_map(|item| match item {
        RawOutputItem::Message { content, .. } => {
            content.iter().find_map(|content| match content {
                RawOutputContent::Text { text } => Some(text.as_str()),
                RawOutputContent::Json { .. } => None,
            })
        }
        RawOutputItem::ToolCall { .. } | RawOutputItem::Reasoning { .. } => None,
    });

    assert!(
        text.is_some_and(|text| !text.trim().is_empty()),
        "expected non-empty assistant text, got: {:?}",
        response.output
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_runner_typed_response_round_trip_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_STRUCTURED_OUTPUT_MODEL).await?;

    let response = runner
        .chat::<(), EchoResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text("Hello!")],
                ModelSelector::from_model(OLLAMA_STRUCTURED_OUTPUT_MODEL),
            )
            .with_max_tokens(64)
            .with_typed_response(TypedResponse::new("echo_response")),
        )
        .await?;

    let structured = response.output.iter().find_map(|item| match item {
        OutputItem::Message { content, .. } => content.iter().find_map(|content| match content {
            OutputContent::Structured { value } => Some(value),
            OutputContent::Text { .. } => None,
        }),
        OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
    });

    assert!(
        structured.is_some_and(|response| !response.value.trim().is_empty()),
        "expected parsed EchoResponse.value to be non-empty, got {:?}",
        response.output
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_runner_decodes_typed_tool_calls_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TOOL_MODEL).await?;

    let response = runner
        .chat::<TestTools, String>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(OLLAMA_TOOL_MODEL),
            )
            .with_max_tokens(128)
            .with_tools(TypedToolSet::new()),
        )
        .await?;

    let tool_calls = response
        .output
        .iter()
        .filter_map(|item| match item {
            OutputItem::ToolCall { call } => Some(call),
            OutputItem::Message { .. } | OutputItem::Reasoning { .. } => None,
        })
        .collect::<Vec<_>>();

    assert!(
        tool_calls.iter().any(
            |call| matches!(call.tool, TestTools::Ping { ref value } if value == "hello-tool")
        ),
        "expected at least one decoded ping tool call, got {:?}",
        response.output
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_runner_streams_typed_response_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_STRUCTURED_OUTPUT_MODEL).await?;

    let mut stream = runner
        .chat_stream::<(), EchoResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text("Hello!")],
                ModelSelector::from_model(OLLAMA_STRUCTURED_OUTPUT_MODEL),
            )
            .with_max_tokens(64)
            .with_response_mode(ResponseMode::Stream)
            .with_typed_response(TypedResponse::new("echo_response")),
        )
        .await?;

    assert_streamed_typed_response(&mut stream).await
}

#[tokio::test]
#[serial]
async fn ollama_runner_streams_typed_tool_calls_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TOOL_MODEL).await?;

    let mut stream = runner
        .chat_stream::<TestTools, String>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(OLLAMA_TOOL_MODEL),
            )
            .with_max_tokens(128)
            .with_response_mode(ResponseMode::Stream)
            .with_tools(TypedToolSet::new()),
        )
        .await?;

    assert_streamed_ping_tool_call(&mut stream).await
}

#[tokio::test]
#[serial]
async fn ollama_provider_replays_completed_tool_exchange_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let provider = ctx.ollama_provider_for_model(OLLAMA_TOOL_MODEL).await?;

    let response = provider
        .chat_raw(RawCompletionRequest {
            model: ModelSelector::from_model(OLLAMA_TOOL_MODEL),
            input: vec![
                RawInputItem::Message {
                    role: Role::User,
                    content: vec![RawInputContent::Text {
                        text: "After the completed tool exchange, reply with exactly the tool result and nothing else."
                            .to_string(),
                    }],
                },
                RawInputItem::ToolCall {
                    call: agents::tools::RawToolCall {
                        id: "call_ping_1".to_string(),
                        name: "ping".to_string(),
                        arguments: json!({ "value": "hello-tool" }),
                    },
                },
                RawInputItem::ToolResult {
                    tool_use_id: "call_ping_1".to_string(),
                    content: "pong:hello-tool".to_string(),
                },
            ],
            temperature: Temperature::Value(0.0),
            top_p: TopP::ProviderDefault,
            top_k: TopK::ProviderDefault,
            token_limit: TokenLimit::Max(64),
            response_mode: ResponseMode::Buffered,
            tools: Some(vec![RawToolDefinition::function(
                "ping",
                Some("Echoes a value as pong:<value>."),
                json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"],
                    "additionalProperties": false
                }),
            )]),
            tool_choice: ToolChoice::ProviderDefault,
            response_format: None,
        })
        .await?;

    let text = response.output.iter().find_map(|item| match item {
        RawOutputItem::Message { content, .. } => {
            content.iter().find_map(|content| match content {
                RawOutputContent::Text { text } => Some(text.as_str()),
                RawOutputContent::Json { .. } => None,
            })
        }
        RawOutputItem::ToolCall { .. } | RawOutputItem::Reasoning { .. } => None,
    });

    assert!(
        text.is_some_and(|text| text.contains("pong:hello-tool")),
        "expected final reply to include replayed tool result, got {:?}",
        response.output
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_runner_typed_response_with_defaulted_nested_fields_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_STRUCTURED_OUTPUT_MODEL).await?;

    let response = runner
        .chat::<(), DefaultedJudgeResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "Return only valid JSON for this exact schema: {\"passed\":true,\"score\":0.5,\"summary\":\"...\",\"evidence\":{\"strengths\":[],\"concerns\":[]}}. All four top-level fields are required. The nested evidence object is required. Both strengths and concerns keys must always be present, even when empty.",
                )],
                ModelSelector::from_model(OLLAMA_STRUCTURED_OUTPUT_MODEL),
            )
            .with_max_tokens(128)
            .with_temperature(0.0)
            .with_typed_response(TypedResponse::new("agent_response")),
        )
        .await?;

    let structured = response.output.iter().find_map(|item| match item {
        OutputItem::Message { content, .. } => content.iter().find_map(|content| match content {
            OutputContent::Structured { value } => Some(value),
            OutputContent::Text { .. } => None,
        }),
        OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
    });

    assert!(
        structured.is_some(),
        "expected defaulted nested structured response, got {:?}",
        response.output
    );
    Ok(())
}
