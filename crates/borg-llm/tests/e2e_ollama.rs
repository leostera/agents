use borg_llm::completion::{CompletionRequest, Message, ModelSelector, RawCompletionRequest};
use borg_llm::error::{Error, LlmResult};
use borg_llm::provider::LlmProvider;
use borg_llm::response::TypedResponse;
use borg_llm::testing::{TestContext, TestProvider};
use borg_llm::tools::{RawToolDefinition, TypedTool, TypedToolSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serial_test::serial;

const OLLAMA_TEXT_MODEL: &str = "qwen2.5:7b";
const OLLAMA_STRUCTURED_OUTPUT_MODEL: &str = "qwen2.5:7b";
const OLLAMA_TOOL_MODEL: &str = "qwen2.5:7b";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EchoResponse {
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
enum TestTools {
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
struct PingArgs {
    value: String,
}

#[tokio::test]
#[serial]
async fn ollama_provider_chat_raw_returns_text_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let provider = ctx.ollama_provider_for_model(OLLAMA_TEXT_MODEL).await?;

    let response = provider
        .chat_raw(RawCompletionRequest {
            model: ModelSelector::from_model(OLLAMA_TEXT_MODEL),
            messages: vec![Message::user(
                "Reply with a short plain-text acknowledgment. Do not return JSON.",
            )],
            temperature: Some(0.0),
            top_p: None,
            top_k: None,
            max_tokens: Some(32),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            response_format: None,
        })
        .await?;

    assert!(
        !response.message.content.trim().is_empty(),
        "expected non-empty assistant text, got: {:?}",
        response.message.content
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
                vec![Message::user("Hello!")],
                ModelSelector::from_model(OLLAMA_STRUCTURED_OUTPUT_MODEL),
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
async fn ollama_runner_decodes_typed_tool_calls_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TOOL_MODEL).await?;

    let response = runner
        .chat::<TestTools, String>(
            CompletionRequest::new(
                vec![Message::user(
                    "You must call the ping tool exactly once. Use the argument value=\"hello-tool\". Do not answer in natural language.",
                )],
                ModelSelector::from_model(OLLAMA_TOOL_MODEL),
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
