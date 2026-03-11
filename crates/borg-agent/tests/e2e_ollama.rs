use borg_agent::{
    Agent, AgentEvent, AgentInput, AgentResult, CallbackToolRunner, ContextManager,
    ExecutionProfile, ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
};
use borg_llm::completion::Temperature;
use borg_llm::completion::{InputItem, ModelSelector, TokenLimit};
use borg_llm::error::LlmResult;
use borg_llm::testing::{TestContext, TestProvider};
use borg_llm::tools::{RawToolDefinition, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serial_test::serial;

const OLLAMA_TEXT_MODEL: &str = "qwen2.5:7b";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EchoResponse {
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
enum TestTools {
    Ping { value: String },
    Redirect { destination: String },
}

impl TypedTool for TestTools {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        vec![
            RawToolDefinition::function(
                "ping",
                Some("Echo a value back to the caller"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"]
                }),
            ),
            RawToolDefinition::function(
                "redirect",
                Some("Redirect work toward a destination"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "destination": { "type": "string" }
                    },
                    "required": ["destination"]
                }),
            ),
        ]
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
            "redirect" => {
                #[derive(Deserialize)]
                struct RedirectArgs {
                    destination: String,
                }

                let args: RedirectArgs = serde_json::from_value(arguments)
                    .map_err(|error| borg_llm::error::Error::parse("tool arguments", error))?;
                Ok(TestTools::Redirect {
                    destination: args.destination,
                })
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

fn ollama_profile() -> ExecutionProfile {
    ExecutionProfile {
        model_selector: ModelSelector::from_model(OLLAMA_TEXT_MODEL),
        temperature: Temperature::Value(0.0),
        token_limit: TokenLimit::Max(64),
        ..ExecutionProfile::default()
    }
}

async fn next_event<M, C, T, R>(
    agent: &mut Agent<M, C, T, R>,
) -> LlmResult<Option<AgentEvent<C, T, R>>>
where
    M: Into<InputItem> + Send + Sync + 'static,
    C: borg_llm::tools::TypedTool + Clone + Serialize + Send + Sync + 'static,
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

fn map_agent_error<T>(result: AgentResult<T>) -> LlmResult<T> {
    result.map_err(|error| borg_llm::error::Error::Internal {
        message: error.to_string(),
    })
}

#[tokio::test]
#[serial]
async fn ollama_agent_send_completes_text_turn_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let mut agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Reply with a short plain-text acknowledgment. Do not return JSON.",
            )),
            ollama_profile(),
        )
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
                "expected non-empty Ollama reply, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_run_streams_text_turn_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .with_execution_profile(ollama_profile())
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.run().await)?;
    tx.send(AgentInput::Message(InputItem::user_text(
        "Reply with a short plain-text acknowledgment. Do not return JSON.",
    )))
    .await
    .expect("send");
    drop(tx);

    assert!(matches!(
        map_agent_error(rx.recv().await.expect("model item"))?,
        AgentEvent::ModelOutputItem { .. }
    ));
    match map_agent_error(rx.recv().await.expect("completed"))? {
        AgentEvent::Completed { reply } => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty Ollama reply, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_static_context_provider_shapes_reply_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let mut agent = Agent::builder()
        .with_context_manager(ContextManager::static_text(
            "Every final answer must start with the exact prefix CTX-OLLAMA: ",
        ))
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Reply with a short plain-text acknowledgment.",
            )),
            ollama_profile(),
        )
        .await
        .expect("turn");

    let _ = next_event(&mut agent).await?;
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.trim_start().starts_with("CTX-OLLAMA:"),
                "expected reply shaped by static context, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_send_twice_reuses_transcript_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let mut agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Remember this exact token for later: borg-agent-stage1. Reply with only OK.",
            )),
            ollama_profile(),
        )
        .await
        .expect("first turn");

    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::ModelOutputItem { .. })
    ));
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(!reply.trim().is_empty());
        }
        other => panic!("expected first completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "What exact token did I ask you to remember? Reply with only the token.",
            )),
            ollama_profile(),
        )
        .await
        .expect("second turn");

    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::ModelOutputItem { .. })
    ));
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("borg-agent-stage1"),
                "expected reply to reuse earlier transcript token, got {:?}",
                reply
            );
        }
        other => panic!("expected second completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_send_decodes_typed_response_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let mut agent = Agent::builder()
        .with_response_type::<EchoResponse>()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Return valid JSON with a non-empty string field named value.",
            )),
            ollama_profile(),
        )
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
                "expected non-empty typed Ollama reply, got {:?}",
                reply
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_executes_ping_tool_and_finishes_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

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
            TestTools::Redirect { destination } => Ok(ToolResultEnvelope {
                call_id: call.call_id,
                result: ToolExecutionResult::Ok {
                    data: Pong {
                        value: format!("redirected:{destination}"),
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
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "First call the ping tool exactly once with value=\"hello-tool\". After receiving the tool result, reply in plain text and include the returned pong value.",
            )),
            ollama_profile(),
        )
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
    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::ModelOutputItem { .. })
    ));
    match next_event(&mut agent).await? {
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

#[tokio::test]
#[serial]
async fn ollama_agent_queues_message_behind_active_turn_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let mut agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Reply with exactly FIRST and nothing else.",
            )),
            ollama_profile(),
        )
        .await
        .expect("first");

    agent
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Reply with exactly SECOND and nothing else.",
            )),
            ollama_profile(),
        )
        .await
        .expect("second");

    match next_event(&mut agent).await? {
        Some(AgentEvent::ModelOutputItem { .. }) => {}
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("first"),
                "expected first queued reply, got {:?}",
                reply
            );
            match next_event(&mut agent).await? {
                Some(AgentEvent::ModelOutputItem { .. }) => {}
                Some(AgentEvent::Completed { reply }) => {
                    assert!(
                        reply.to_lowercase().contains("second"),
                        "expected second queued reply, got {:?}",
                        reply
                    );
                    assert!(next_event(&mut agent).await?.is_none());
                    return Ok(());
                }
                other => panic!("expected second queued turn event, got {other:?}"),
            }
        }
        other => panic!("expected first queued turn event, got {other:?}"),
    }

    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("first"),
                "expected first queued reply, got {:?}",
                reply
            );
        }
        other => panic!("expected first completed event, got {other:?}"),
    }
    match next_event(&mut agent).await? {
        Some(AgentEvent::ModelOutputItem { .. }) => {}
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("second"),
                "expected second queued reply, got {:?}",
                reply
            );
            assert!(next_event(&mut agent).await?.is_none());
            return Ok(());
        }
        other => panic!("expected second queued turn event, got {other:?}"),
    }
    match next_event(&mut agent).await? {
        Some(AgentEvent::Completed { reply }) => {
            assert!(
                reply.to_lowercase().contains("second"),
                "expected second queued reply, got {:?}",
                reply
            );
        }
        other => panic!("expected second completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_steer_clears_pending_tool_plan_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

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
            TestTools::Redirect { destination } => Ok(ToolResultEnvelope {
                call_id: call.call_id,
                result: ToolExecutionResult::Ok {
                    data: Pong {
                        value: format!("redirected:{destination}"),
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
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "First call the ping tool exactly once with value=\"hello-tool\". Then explain the result.",
            )),
            ollama_profile(),
        )
        .await
        .expect("turn");

    match next_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call }) => {
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    agent
        .send_with_profile(
            AgentInput::Steer(InputItem::user_text(
                "IMPORTANT: Ignore the previous request to call ping. Instead call the redirect tool exactly once with destination=\"rerouted-path\" and then reply with exactly STEERED.",
            )),
            ollama_profile(),
        )
        .await
        .expect("steer");

    let rerouted_call_id = match next_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call }) => {
            let call_id = call.call_id;
            assert_eq!(
                call.call,
                TestTools::Redirect {
                    destination: "rerouted-path".to_string()
                }
            );
            call_id
        }
        Some(AgentEvent::ToolExecutionCompleted { .. }) => {
            panic!("steering should interrupt the pending ping tool execution before it completes");
        }
        other => panic!("expected rerouted tool call event, got {other:?}"),
    };
    match next_event(&mut agent).await? {
        Some(AgentEvent::ToolExecutionCompleted { result }) => {
            assert_eq!(result.call_id, rerouted_call_id);
            match result.result {
                ToolExecutionResult::Ok { data } => {
                    assert_eq!(
                        data,
                        Pong {
                            value: "redirected:rerouted-path".to_string()
                        }
                    );
                }
                ToolExecutionResult::Error { message } => {
                    panic!("unexpected rerouted tool error: {message}");
                }
            }
        }
        other => panic!("expected rerouted tool execution event, got {other:?}"),
    }

    agent
        .send(AgentInput::Cancel)
        .await
        .expect("cancel rerouted turn");
    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::Cancelled)
    ));
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_cancel_during_active_turn_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

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
            TestTools::Redirect { destination } => Ok(ToolResultEnvelope {
                call_id: call.call_id,
                result: ToolExecutionResult::Ok {
                    data: Pong {
                        value: format!("redirected:{destination}"),
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
        .send_with_profile(
            AgentInput::Message(InputItem::user_text(
                "Call the ping tool exactly once with value=\"hello-tool\" and then explain it.",
            )),
            ollama_profile(),
        )
        .await
        .expect("turn");

    match next_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { .. }) => {}
        other => panic!("expected tool call event, got {other:?}"),
    }

    agent.send(AgentInput::Cancel).await.expect("cancel");

    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::Cancelled)
    ));
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}
