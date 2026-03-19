#![cfg(feature = "live-provider-tests")]

mod common;

use agents::completion::{
    CompletionRequest, InputItem, ModelSelector, OutputContent, OutputItem, RawCompletionRequest,
    RawInputContent, RawInputItem, RawOutputContent, RawOutputItem, ResponseMode, Role,
    Temperature, TokenLimit, ToolChoice, TopK, TopP,
};
use agents::error::LlmResult;
use agents::provider::LlmProvider;
use agents::response::TypedResponse;
use agents::tools::TypedToolSet;
use agents_test::{openrouter_provider_for_model, optional_test_env, runner_with_openrouter_model};
use common::{
    EchoResponse, TestTools, assert_completion_usage_reported, assert_streamed_ping_tool_call,
    assert_streamed_typed_response,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn openrouter_model() -> String {
    optional_test_env("OPENROUTER_MODEL")
        .expect("OPENROUTER_MODEL must be set for OpenRouter e2e tests")
}

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
async fn openrouter_provider_chat_raw_returns_text_long() -> LlmResult<()> {
    let model = openrouter_model();
    let provider = openrouter_provider_for_model(&model)?;

    let response = provider
        .chat_raw(RawCompletionRequest {
            model: ModelSelector::from_model(model),
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
        "expected non-empty assistant text, got {:?}",
        response.output
    );
    Ok(())
}

#[tokio::test]
async fn openrouter_runner_typed_response_round_trip_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let response = runner
        .chat::<(), EchoResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "Return valid JSON with a non-empty string field named value.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(64)
            .with_typed_response(TypedResponse::new("echo_response")),
        )
        .await?;
    assert_completion_usage_reported(&response);

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
async fn openrouter_runner_decodes_typed_tool_calls_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let response = runner
        .chat::<TestTools, String>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(128)
            .with_tools(TypedToolSet::new()),
        )
        .await?;
    assert_completion_usage_reported(&response);

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
async fn openrouter_runner_streams_typed_response_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let mut stream = runner
        .chat_stream::<(), EchoResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "Return valid JSON with a non-empty string field named value.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(64)
            .with_response_mode(ResponseMode::Stream)
            .with_typed_response(TypedResponse::new("echo_response")),
        )
        .await?;

    assert_streamed_typed_response(&mut stream).await
}

#[tokio::test]
async fn openrouter_runner_streams_typed_tool_calls_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let mut stream = runner
        .chat_stream::<TestTools, String>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(128)
            .with_response_mode(ResponseMode::Stream)
            .with_tools(TypedToolSet::new()),
        )
        .await?;

    assert_streamed_ping_tool_call(&mut stream).await
}

#[tokio::test]
async fn openrouter_runner_typed_response_with_defaulted_nested_fields_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let response = runner
        .chat::<(), DefaultedJudgeResponse>(
            CompletionRequest::new(
                vec![InputItem::user_text(
                    "Return valid JSON with fields passed, score, summary, and nested evidence containing strengths and concerns arrays.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(128)
            .with_typed_response(TypedResponse::new("agent_response")),
        )
        .await?;
    assert_completion_usage_reported(&response);

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
