use agents::completion::InputItem;
use agents::error::LlmResult;
use agents::tools::{RawToolDefinition, TypedTool};
use agents::{
    AgentEvent, AgentInput, AgentResult, CallbackToolRunner, SessionAgent as Agent,
    ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
};
use agents_test::{optional_test_env, runner_with_anthropic_model};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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
                    "properties": { "value": { "type": "string" } },
                    "required": ["value"]
                }),
            ),
            RawToolDefinition::function(
                "redirect",
                Some("Redirect work toward a destination"),
                serde_json::json!({
                    "type": "object",
                    "properties": { "destination": { "type": "string" } },
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
                    .map_err(|error| agents::error::Error::parse("tool arguments", error))?;
                Ok(TestTools::Ping { value: args.value })
            }
            "redirect" => {
                #[derive(Deserialize)]
                struct RedirectArgs {
                    destination: String,
                }

                let args: RedirectArgs = serde_json::from_value(arguments)
                    .map_err(|error| agents::error::Error::parse("tool arguments", error))?;
                Ok(TestTools::Redirect {
                    destination: args.destination,
                })
            }
            other => Err(agents::error::Error::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Pong {
    value: String,
}

fn anthropic_model() -> String {
    optional_test_env("BORG_TEST_ANTHROPIC_MODEL")
        .expect("BORG_TEST_ANTHROPIC_MODEL must be set for Anthropic agent e2e tests")
}

fn map_agent_error<T>(result: AgentResult<T>) -> LlmResult<T> {
    result.map_err(|error| agents::error::Error::Internal {
        message: error.to_string(),
    })
}

async fn next_event<M, C, T, R>(
    agent: &mut Agent<M, C, T, R>,
) -> LlmResult<Option<AgentEvent<C, T, R>>>
where
    M: Clone + Serialize + DeserializeOwned + Into<InputItem> + Send + Sync + 'static,
    C: TypedTool + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    R: Clone + Serialize + for<'de> Deserialize<'de> + JsonSchema + Send + Sync + 'static,
{
    agent
        .next()
        .await
        .map_err(|error| agents::error::Error::Internal {
            message: error.to_string(),
        })
}

async fn next_nonempty_event<M, C, T, R>(
    agent: &mut Agent<M, C, T, R>,
) -> LlmResult<Option<AgentEvent<C, T, R>>>
where
    M: Clone + Serialize + DeserializeOwned + Into<InputItem> + Send + Sync + 'static,
    C: TypedTool + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    R: Clone + Serialize + for<'de> Deserialize<'de> + JsonSchema + Send + Sync + 'static,
{
    loop {
        match next_event(agent).await? {
            Some(AgentEvent::ModelOutputItem {
                item: agents::completion::OutputItem::Message { content, .. },
                ..
            }) if content.iter().all(|part| match part {
                agents::completion::OutputContent::Text { text } => text.trim().is_empty(),
                agents::completion::OutputContent::Structured { .. } => false,
            }) => {}
            other => return Ok(other),
        }
    }
}

#[tokio::test]
async fn anthropic_agent_send_completes_text_turn_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
    let mut agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
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
        Some(AgentEvent::Completed { reply, .. }) => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty reply, got {reply:?}"
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_run_streams_text_turn_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
    let agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.spawn().await)?;
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
        AgentEvent::Completed { reply, .. } => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty reply, got {reply:?}"
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_send_decodes_typed_response_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
    let mut agent = Agent::raw_builder()
        .with_response_type::<EchoResponse>()
        .with_llm_runner(runner.into())
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
        Some(AgentEvent::Completed { reply, .. }) => {
            assert!(
                !reply.value.trim().is_empty(),
                "expected non-empty reply, got {reply:?}"
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_executes_ping_tool_and_finishes_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let mut agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "First call the ping tool exactly once with value=\"hello-tool\". Do not explain the plan before calling it, and do not call the tool more than once. After receiving the tool result, reply in plain text and include the returned pong value.",
        )))
        .await
        .expect("turn");

    let tool_call_id = match next_nonempty_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call, .. }) => {
            let call_id = call.call_id;
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
            call_id
        }
        Some(AgentEvent::ModelOutputItem { .. }) => match next_nonempty_event(&mut agent).await? {
            Some(AgentEvent::ToolCallRequested { call, .. }) => {
                let call_id = call.call_id;
                assert_eq!(
                    call.call,
                    TestTools::Ping {
                        value: "hello-tool".to_string()
                    }
                );
                call_id
            }
            other => panic!("expected tool call event after initial model output, got {other:?}"),
        },
        other => panic!("expected tool call event, got {other:?}"),
    };

    match next_nonempty_event(&mut agent).await? {
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
                    panic!("unexpected tool error: {message}")
                }
            }
        }
        other => panic!("expected tool execution event, got {other:?}"),
    }

    agent
        .send(AgentInput::Steer(InputItem::user_text(
            "Do not call any more tools. Using the tool result you already have, reply in plain text and include the returned pong value.",
        )))
        .await
        .expect("steer");

    match next_nonempty_event(&mut agent).await? {
        Some(AgentEvent::ModelOutputItem { .. }) => match next_event(&mut agent).await? {
            Some(AgentEvent::Completed { reply, .. }) => {
                assert!(
                    reply.to_lowercase().contains("pong:hello-tool"),
                    "expected tool output, got {reply:?}"
                );
            }
            other => panic!("expected completed event after model output, got {other:?}"),
        },
        Some(AgentEvent::Completed { reply, .. }) => {
            assert!(
                reply.to_lowercase().contains("pong:hello-tool"),
                "expected tool output, got {reply:?}"
            );
        }
        other => panic!("expected completed event, got {other:?}"),
    }
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_run_executes_ping_tool_and_finishes_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.spawn().await)?;
    tx.send(AgentInput::Message(InputItem::user_text(
        "First call the ping tool exactly once with value=\"hello-tool\". Do not explain the plan before calling it, and do not call the tool more than once. After receiving the tool result, reply in plain text and include the returned pong value.",
    )))
    .await
    .expect("send");
    drop(tx);

    let first = map_agent_error(rx.recv().await.expect("first event"))?;
    let tool_call_id = match first {
        AgentEvent::ToolCallRequested { call, .. } => {
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
            call.call_id
        }
        AgentEvent::ModelOutputItem { .. } => {
            match map_agent_error(rx.recv().await.expect("tool call"))? {
                AgentEvent::ToolCallRequested { call, .. } => {
                    assert_eq!(
                        call.call,
                        TestTools::Ping {
                            value: "hello-tool".to_string()
                        }
                    );
                    call.call_id
                }
                other => {
                    panic!("expected tool call event after initial model output, got {other:?}")
                }
            }
        }
        other => panic!("expected tool call event, got {other:?}"),
    };

    match map_agent_error(rx.recv().await.expect("tool result"))? {
        AgentEvent::ToolExecutionCompleted { result } => {
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
                other => panic!("expected successful tool result, got {other:?}"),
            }
        }
        other => panic!("expected tool execution event, got {other:?}"),
    }

    let completed = loop {
        match map_agent_error(rx.recv().await.expect("follow-up event"))? {
            AgentEvent::ModelOutputItem { .. } => continue,
            AgentEvent::Completed { reply, .. } => break reply,
            other => panic!("expected final reply after tool execution, got {other:?}"),
        }
    };
    assert!(
        completed.contains("pong:hello-tool"),
        "expected final reply to include tool output, got {completed:?}"
    );
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_run_queues_messages_in_order_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
    let agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.spawn().await)?;
    tx.send(AgentInput::Message(InputItem::user_text(
        "Reply with exactly FIRST and nothing else.",
    )))
    .await
    .expect("first send");
    tx.send(AgentInput::Message(InputItem::user_text(
        "Reply with exactly SECOND and nothing else.",
    )))
    .await
    .expect("second send");
    drop(tx);

    let first_reply = loop {
        match map_agent_error(rx.recv().await.expect("first turn event"))? {
            AgentEvent::ModelOutputItem { .. } => continue,
            AgentEvent::Completed { reply, .. } => break reply,
            other => panic!("expected first completed event, got {other:?}"),
        }
    };
    assert!(
        first_reply.to_lowercase().contains("first"),
        "expected first queued reply, got {first_reply:?}"
    );

    let second_reply = loop {
        match map_agent_error(rx.recv().await.expect("second turn event"))? {
            AgentEvent::ModelOutputItem { .. } => continue,
            AgentEvent::Completed { reply, .. } => break reply,
            other => panic!("expected second completed event, got {other:?}"),
        }
    };
    assert!(
        second_reply.to_lowercase().contains("second"),
        "expected second queued reply, got {second_reply:?}"
    );
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_run_cancels_active_turn_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.spawn().await)?;
    tx.send(AgentInput::Message(InputItem::user_text(
        "Call the ping tool exactly once with value=\"hello-tool\". Do not explain the plan before calling it.",
    )))
    .await
    .expect("send");

    match map_agent_error(rx.recv().await.expect("first event"))? {
        AgentEvent::ToolCallRequested { .. } => {}
        AgentEvent::ModelOutputItem { .. } => {
            match map_agent_error(rx.recv().await.expect("tool call"))? {
                AgentEvent::ToolCallRequested { .. } => {}
                other => {
                    panic!("expected tool call event after initial model output, got {other:?}")
                }
            }
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    tx.send(AgentInput::Cancel).await.expect("cancel");
    drop(tx);

    match map_agent_error(rx.recv().await.expect("post-cancel event"))? {
        AgentEvent::Cancelled => {}
        AgentEvent::ToolExecutionCompleted { .. } => {
            assert!(matches!(
                map_agent_error(rx.recv().await.expect("cancelled"))?,
                AgentEvent::Cancelled
            ));
        }
        other => panic!("expected cancellation path event, got {other:?}"),
    }
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_run_steer_clears_pending_tool_plan_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    let (tx, mut rx) = map_agent_error(agent.spawn().await)?;
    tx.send(AgentInput::Message(InputItem::user_text(
        "First call the ping tool exactly once with value=\"hello-tool\". Then explain the result.",
    )))
    .await
    .expect("send");

    match map_agent_error(rx.recv().await.expect("first event"))? {
        AgentEvent::ToolCallRequested { call, .. } => {
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
        }
        AgentEvent::ModelOutputItem { .. } => {
            match map_agent_error(rx.recv().await.expect("tool call"))? {
                AgentEvent::ToolCallRequested { call, .. } => {
                    assert_eq!(
                        call.call,
                        TestTools::Ping {
                            value: "hello-tool".to_string()
                        }
                    );
                }
                other => {
                    panic!("expected tool call event after initial model output, got {other:?}")
                }
            }
        }
        other => panic!("expected tool call event, got {other:?}"),
    }

    tx.send(AgentInput::Steer(InputItem::user_text(
        "IMPORTANT: The previous ping tool call was interrupted and must not be resumed. Do not call ping again. Your only allowed next action is to call the redirect tool exactly once with destination=\"rerouted-path\", then reply with exactly STEERED.",
    )))
    .await
    .expect("steer");

    let rerouted_call_id = match map_agent_error(rx.recv().await.expect("rerouted event"))? {
        AgentEvent::ToolCallRequested { call, .. } => {
            let call_id = call.call_id;
            assert_eq!(
                call.call,
                TestTools::Redirect {
                    destination: "rerouted-path".to_string()
                }
            );
            call_id
        }
        AgentEvent::ModelOutputItem { .. } => {
            match map_agent_error(rx.recv().await.expect("rerouted tool call"))? {
                AgentEvent::ToolCallRequested { call, .. } => {
                    let call_id = call.call_id;
                    assert_eq!(
                        call.call,
                        TestTools::Redirect {
                            destination: "rerouted-path".to_string()
                        }
                    );
                    call_id
                }
                other => {
                    panic!("expected rerouted tool call event after steering output, got {other:?}")
                }
            }
        }
        AgentEvent::ToolExecutionCompleted { .. } => {
            panic!("steering should interrupt the pending ping tool execution before it completes");
        }
        other => panic!("expected rerouted tool call event, got {other:?}"),
    };

    match map_agent_error(rx.recv().await.expect("rerouted tool result"))? {
        AgentEvent::ToolExecutionCompleted { result } => {
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
                other => panic!("expected successful rerouted tool result, got {other:?}"),
            }
        }
        other => panic!("expected rerouted tool execution event, got {other:?}"),
    }

    tx.send(AgentInput::Cancel).await.expect("cancel");
    drop(tx);
    assert!(matches!(
        map_agent_error(rx.recv().await.expect("cancelled"))?,
        AgentEvent::Cancelled
    ));
    assert!(rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_steer_clears_pending_tool_plan_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let mut agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "First call the ping tool exactly once with value=\"hello-tool\". Then explain the result.",
        )))
        .await
        .expect("turn");

    match next_nonempty_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call, .. }) => {
            assert_eq!(
                call.call,
                TestTools::Ping {
                    value: "hello-tool".to_string()
                }
            );
        }
        Some(AgentEvent::ModelOutputItem { .. }) => match next_nonempty_event(&mut agent).await? {
            Some(AgentEvent::ToolCallRequested { call, .. }) => {
                assert_eq!(
                    call.call,
                    TestTools::Ping {
                        value: "hello-tool".to_string()
                    }
                );
            }
            other => panic!("expected tool call event after initial model output, got {other:?}"),
        },
        other => panic!("expected tool call event, got {other:?}"),
    }

    agent
        .send(AgentInput::Steer(InputItem::user_text(
            "IMPORTANT: The previous ping tool call was interrupted and must not be resumed. Do not call ping again. Your only allowed next action is to call the redirect tool exactly once with destination=\"rerouted-path\", then reply with exactly STEERED.",
        )))
        .await
        .expect("steer");

    let rerouted_call_id = match next_nonempty_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { call, .. }) => {
            let call_id = call.call_id;
            assert_eq!(
                call.call,
                TestTools::Redirect {
                    destination: "rerouted-path".to_string(),
                }
            );
            call_id
        }
        Some(AgentEvent::ModelOutputItem { .. }) => match next_nonempty_event(&mut agent).await? {
            Some(AgentEvent::ToolCallRequested { call, .. }) => {
                let call_id = call.call_id;
                assert_eq!(
                    call.call,
                    TestTools::Redirect {
                        destination: "rerouted-path".to_string(),
                    }
                );
                call_id
            }
            other => {
                panic!("expected rerouted tool call event after steering output, got {other:?}")
            }
        },
        Some(AgentEvent::ToolExecutionCompleted { .. }) => {
            panic!("steering should interrupt the pending ping tool execution before it completes");
        }
        other => panic!("expected rerouted tool call event, got {other:?}"),
    };

    match next_nonempty_event(&mut agent).await? {
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
                    panic!("unexpected rerouted tool error: {message}")
                }
            }
        }
        other => panic!("expected rerouted tool execution event, got {other:?}"),
    }

    agent.send(AgentInput::Cancel).await.expect("cancel");
    assert!(matches!(
        next_event(&mut agent).await?,
        Some(AgentEvent::Cancelled)
    ));
    assert!(next_event(&mut agent).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn anthropic_agent_cancel_during_active_turn_long() -> LlmResult<()> {
    let runner = runner_with_anthropic_model(&anthropic_model())?;
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

    let mut agent = Agent::raw_builder()
        .with_llm_runner(runner.into())
        .with_tool_runner(tool_runner)
        .build()
        .expect("agent");

    agent
        .send(AgentInput::Message(InputItem::user_text(
            "Call the ping tool exactly once with value=\"hello-tool\". Do not explain the plan before calling it.",
        )))
        .await
        .expect("turn");

    match next_nonempty_event(&mut agent).await? {
        Some(AgentEvent::ToolCallRequested { .. }) => {}
        Some(AgentEvent::ModelOutputItem { .. }) => match next_nonempty_event(&mut agent).await? {
            Some(AgentEvent::ToolCallRequested { .. }) => {}
            other => panic!("expected tool call event after initial model output, got {other:?}"),
        },
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
