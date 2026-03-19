mod common;

use agents::completion::{
    CompletionEvent, CompletionRequest, InputItem, ModelSelector, OutputContent, OutputItem,
    RawCompletionRequest, RawInputContent, RawInputItem, RawOutputContent, RawOutputItem,
    ResponseMode, Role, Temperature, TokenLimit, ToolChoice, TopK, TopP,
};
use agents::error::LlmResult;
use agents::provider::LlmProvider;
use agents::response::TypedResponse;
use agents::tools::TypedToolSet;
use agents_test::{optional_test_env, runner_with_workers_ai_model, workers_ai_provider_for_model};
use common::{EchoResponse, TestTools, assert_completion_usage_reported};

fn workers_ai_model() -> String {
    optional_test_env("BORG_TEST_WORKERS_AI_MODEL")
        .or_else(|| optional_test_env("BORG_LLM_WORKERS_AI_MODEL"))
        .expect("BORG_TEST_WORKERS_AI_MODEL must be set for Workers AI e2e tests")
}

fn workers_ai_tool_model() -> Option<String> {
    optional_test_env("BORG_TEST_WORKERS_AI_TOOL_MODEL")
}

#[tokio::test]
async fn workers_ai_provider_chat_raw_returns_text_long() -> LlmResult<()> {
    let model = workers_ai_model();
    let provider = workers_ai_provider_for_model(&model)?;

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
    assert_eq!(response.provider, agents::ProviderType::WorkersAI);
    Ok(())
}

#[tokio::test]
async fn workers_ai_runner_typed_response_round_trip_long() -> LlmResult<()> {
    let model = workers_ai_model();
    let runner = runner_with_workers_ai_model(&model)?;

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
async fn workers_ai_runner_decodes_typed_tool_calls_long() -> LlmResult<()> {
    let Some(model) = workers_ai_tool_model() else {
        return Ok(());
    };
    let runner = runner_with_workers_ai_model(&model)?;

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
async fn workers_ai_runner_streams_typed_response_long() -> LlmResult<()> {
    let model = workers_ai_model();
    let runner = runner_with_workers_ai_model(&model)?;

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

    let mut final_response = None;

    while let Some(event) = stream.recv().await {
        match event? {
            CompletionEvent::Done(response) => {
                common::assert_usage_reported(&response.usage);
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
            CompletionEvent::TextDelta { .. }
            | CompletionEvent::ReasoningDelta { .. }
            | CompletionEvent::ToolCall { .. } => {}
        }
    }

    let final_response = final_response.expect("expected final streamed typed response");
    assert!(
        !final_response.value.trim().is_empty(),
        "expected non-empty typed streamed response value, got {:?}",
        final_response
    );

    Ok(())
}

#[tokio::test]
async fn workers_ai_runner_streams_typed_tool_calls_long() -> LlmResult<()> {
    let Some(model) = workers_ai_tool_model() else {
        return Ok(());
    };
    let runner = runner_with_workers_ai_model(&model)?;

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

    let mut final_tool_calls = Vec::new();

    while let Some(event) = stream.recv().await {
        match event? {
            CompletionEvent::Done(response) => {
                common::assert_usage_reported(&response.usage);
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
            CompletionEvent::TextDelta { .. }
            | CompletionEvent::ReasoningDelta { .. }
            | CompletionEvent::ToolCall { .. } => {}
        }
    }

    assert!(
        final_tool_calls.iter().any(
            |call| matches!(call.tool, TestTools::Ping { ref value } if value == "hello-tool")
        ),
        "expected final decoded ping tool call, got {:?}",
        final_tool_calls
    );

    Ok(())
}
