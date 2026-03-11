use borg_agent::{
    Agent, AgentEvent, AgentInput, CallbackToolRunner, ToolCallEnvelope, ToolExecutionResult,
    ToolResultEnvelope,
};
use borg_llm::completion::InputItem;
use borg_llm::error::LlmResult;
use borg_llm::testing::{optional_test_env, runner_with_openrouter_model};
use borg_llm::tools::{RawToolDefinition, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
            "ping" => {
                #[derive(Deserialize)]
                struct PingArgs {
                    value: String,
                }

                let args: PingArgs = serde_json::from_value(arguments)
                    .map_err(|error| borg_llm::error::Error::parse("tool arguments", error))?;
                Ok(TestTools::Ping { value: args.value })
            }
            other => Err(borg_llm::error::Error::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Pong {
    value: String,
}

fn openrouter_model() -> String {
    optional_test_env("BORG_TEST_OPENROUTER_MODEL")
        .expect("BORG_TEST_OPENROUTER_MODEL must be set for OpenRouter e2e tests")
}

async fn next_event<M, C, T, R>(
    agent: &mut Agent<M, C, T, R>,
) -> LlmResult<Option<AgentEvent<C, T, R>>>
where
    M: Into<InputItem> + Send + 'static,
    C: TypedTool + Clone + Send + Sync + 'static,
    T: Clone + Serialize + Send + Sync + 'static,
    R: Clone + Serialize + for<'de> Deserialize<'de> + JsonSchema + Send + Sync + 'static,
{
    agent
        .next()
        .await
        .map_err(|error| borg_llm::error::Error::Internal {
            message: error.to_string(),
        })
}

#[tokio::test]
async fn openrouter_agent_send_completes_text_turn_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let mut agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "Reply with a short plain-text acknowledgment. Do not return JSON.",
        )))
        .await
        .expect("turn");

    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::ModelOutputItem { .. })
    ));
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty OpenRouter reply, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn openrouter_agent_send_decodes_typed_response_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let mut agent = Agent::builder()
        .with_response_type::<EchoResponse>()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "Return valid JSON with a non-empty string field named value.",
        )))
        .await
        .expect("turn");

    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::ModelOutputItem { .. })
    ));
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                !reply.value.trim().is_empty(),
                "expected non-empty typed OpenRouter reply, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn openrouter_agent_executes_ping_tool_and_finishes_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let tool_runner = CallbackToolRunner::new(|call: ToolCallEnvelope<TestTools>| async move {
        match call.call {
            TestTools::Ping { value } => Ok(ToolResultEnvelope {
                call_id: call.call_id,
                result: ToolExecutionResult::Ok {
                    data: Pong {
                        value: format!("pong:{value}"),
                    },
                },
            }),
        }
    });

    let mut agent = Agent::builder()
        .with_llm_runner(runner)
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "First call the ping tool exactly once with value=\"hello-tool\". After receiving the tool result, reply in plain text and include the returned pong value.",
        )))
        .await
        .expect("turn");

    let tool_call_id = match next_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call }) => {
            let call_id = call.call_id;
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
            call_id
        }
        other => panic!("expected tool call event, got {other:?}"),
    };
    match next_event(&mut agent).await? {
        Some(AgentEvent::ToolExecutionCompleted { result }) => {
            assert_eq!(result.call_id, tool_call_id);
            match result.result {
                ToolExecutionResult::Ok { data } => {
                    assert_eq!(
                        data,
                        Pong {
                            value: "pong:hello-tool".to_string()
                        }
                    );
                }
                ToolExecutionResult::Error { message } => {
                    panic!("unexpected tool error: {message}");
                }
            }
        }
        other => panic!("expected tool execution event, got {other:?}"),
    }
    match next_event(&mut agent).await? {
        Some(AgentEvent::ModelOutputItem { .. }) => match next_event(&mut agent).await? {
            Some(AgentEvent::Completed { reply }) => {
                assert!(
                    reply.to_lowercase().contains("pong:hello-tool"),
                    "expected final reply to include tool output, got {:?}",
                    reply
                );
            }
            other => panic!("expected completed event after model output, got {other:?}"),
        },
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("pong:hello-tool"),
                "expected final reply to include tool output, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}
