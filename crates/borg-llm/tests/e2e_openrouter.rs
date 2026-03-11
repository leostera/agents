mod common;

use borg_llm::completion::{
    CompletionRequest, Message, ModelSelector, RawCompletionRequest, ResponseMode, Temperature,
    TokenLimit, ToolChoice, TopK, TopP,
};
use borg_llm::error::LlmResult;
use borg_llm::provider::LlmProvider;
use borg_llm::response::TypedResponse;
use borg_llm::testing::{
    openrouter_provider_for_model, optional_test_env, runner_with_openrouter_model,
};
use borg_llm::tools::TypedToolSet;
use common::{EchoResponse, TestTools, assert_streamed_typed_response};
use serial_test::serial;

fn openrouter_model() -> String {
    optional_test_env("BORG_TEST_OPENROUTER_MODEL")
        .expect("BORG_TEST_OPENROUTER_MODEL must be set for OpenRouter e2e tests")
}

#[tokio::test]
#[serial]
async fn openrouter_provider_chat_raw_returns_text_long() -> LlmResult<()> {
    let model = openrouter_model();
    let provider = openrouter_provider_for_model(&model)?;

    let response = provider
        .chat_raw(RawCompletionRequest {
            model: ModelSelector::from_model(model),
            messages: vec![Message::user(
                "Reply with a short plain-text acknowledgment. Do not return JSON.",
            )],
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

    assert!(
        !response.message.content.trim().is_empty(),
        "expected non-empty assistant text, got {:?}",
        response.message.content
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn openrouter_runner_typed_response_round_trip_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let response = runner
        .chat::<(), EchoResponse>(
            CompletionRequest::new(
                vec![Message::user(
                    "Return valid JSON with a non-empty string field named value.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(64)
            .with_typed_response(TypedResponse::new("echo_response")),
        )
        .await?;

    assert!(
        !response.message.content.value.trim().is_empty(),
        "expected parsed EchoResponse.value to be non-empty, got {:?}",
        response.message.content
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn openrouter_runner_decodes_typed_tool_calls_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let response = runner
        .chat::<TestTools, String>(
            CompletionRequest::new(
                vec![Message::user(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(model),
            )
            .with_max_tokens(128)
            .with_tools(TypedToolSet::new()),
        )
        .await?;

    assert!(
        response.tool_calls.iter().any(
            |call| matches!(call.tool, TestTools::Ping { ref value } if value == "hello-tool")
        ),
        "expected at least one decoded ping tool call, got {:?}",
        response.tool_calls
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn openrouter_runner_streams_typed_response_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let mut stream = runner
        .chat_stream::<(), EchoResponse>(
            CompletionRequest::new(
                vec![Message::user(
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
