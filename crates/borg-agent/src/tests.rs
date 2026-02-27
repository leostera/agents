use std::path::PathBuf;

use super::{
    Agent, AgentTools, Message, Session, SessionResult, Tool, ToolRequest, ToolResponse,
    ToolResultData, ToolRunner, ToolSpec, Toolchain, call_tool, to_provider_messages,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::uri;
use borg_db::BorgDb;
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, StopReason, TranscriptionRequest,
};
use serde_json::{Value, json};
use std::sync::Once;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, trace};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

struct ScriptedRunner {
    calls_tx: mpsc::UnboundedSender<ToolRequest>,
    outputs_rx: Mutex<mpsc::UnboundedReceiver<Result<ToolResponse, String>>>,
}

#[async_trait]
impl ToolRunner for ScriptedRunner {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
        trace!(
            target: "borg_agent_test",
            tool_call_id = request.tool_call_id.as_str(),
            tool_name = request.tool_name.as_str(),
            "scripted runner received tool request"
        );
        self.calls_tx
            .send(request)
            .map_err(|e| anyhow!(e.to_string()))?;
        self.outputs_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow!("missing scripted tool output"))?
            .map_err(|e| anyhow!(e))
    }
}

struct ScriptedProvider {
    requests_tx: mpsc::UnboundedSender<LlmRequest>,
    responses_rx: Mutex<mpsc::UnboundedReceiver<Result<LlmAssistantMessage, String>>>,
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        trace!(
            target: "borg_agent_test",
            model = req.model.as_str(),
            message_count = req.messages.len(),
            "scripted provider received chat request"
        );
        self.requests_tx
            .send(req.clone())
            .map_err(|e| anyhow!(e.to_string()))?;
        self.responses_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow!("missing scripted llm response"))?
            .map_err(|e| anyhow!(e))
    }

    async fn transcribe(&self, _req: &TranscriptionRequest) -> Result<String> {
        Err(anyhow!("transcribe not supported in scripted provider"))
    }
}

fn scripted_provider(
    responses: Vec<Result<LlmAssistantMessage, String>>,
) -> (ScriptedProvider, mpsc::UnboundedReceiver<LlmRequest>) {
    let (requests_tx, requests_rx) = mpsc::unbounded_channel();
    let (responses_tx, responses_rx) = mpsc::unbounded_channel();
    for response in responses {
        responses_tx.send(response).unwrap();
    }
    drop(responses_tx);
    (
        ScriptedProvider {
            requests_tx,
            responses_rx: Mutex::new(responses_rx),
        },
        requests_rx,
    )
}

fn scripted_runner(
    outputs: Vec<Result<ToolResponse, String>>,
) -> (ScriptedRunner, mpsc::UnboundedReceiver<ToolRequest>) {
    let (calls_tx, calls_rx) = mpsc::unbounded_channel();
    let (outputs_tx, outputs_rx) = mpsc::unbounded_channel();
    for output in outputs {
        outputs_tx.send(output).unwrap();
    }
    drop(outputs_tx);
    (
        ScriptedRunner {
            calls_tx,
            outputs_rx: Mutex::new(outputs_rx),
        },
        calls_rx,
    )
}

fn assistant_text(text: &str) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: vec![ProviderBlock::Text(text.to_string())],
        stop_reason: StopReason::EndOfTurn,
        error_message: None,
    }
}

fn assistant_tool_calls(calls: Vec<(&str, &str, Value)>) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: calls
            .into_iter()
            .map(|(id, name, args)| ProviderBlock::ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments_json: args,
            })
            .collect(),
        stop_reason: StopReason::ToolCall,
        error_message: None,
    }
}

fn init_test_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    EnvFilter::new("info,borg_agent=debug,borg_agent_test=trace")
                }),
            )
            .with_test_writer()
            .try_init()
            .ok();
    });
}

async fn make_test_db() -> Result<BorgDb> {
    let path = PathBuf::from(format!("/tmp/borg-agent-test-{}.db", Uuid::now_v7()));
    debug!(
        target: "borg_agent_test",
        path = %path.display(),
        "opening temporary unit-test db"
    );
    let borg_db = BorgDb::open_local(path.to_string_lossy().as_ref()).await?;
    borg_db.migrate().await?;
    trace!(target: "borg_agent_test", "unit-test db migrated");
    Ok(borg_db)
}

async fn make_session() -> Result<(Agent, Session)> {
    init_test_tracing();
    let db = make_test_db().await?;
    let agent = Agent::new(uri!("borg", "agent", "test-agent")).with_system_prompt("system prompt");
    let session = Session::new(uri!("borg", "session", "test-session"), agent.clone(), db).await?;
    debug!(target: "borg_agent_test", "created test agent session");
    Ok((agent, session))
}

#[tokio::test]
async fn a1_no_tool_completion() {
    info!(target: "borg_agent_test", test = "a1_no_tool_completion", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "hello".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![Ok(assistant_text("hello back"))]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    info!(target: "borg_agent_test", test = "a1_no_tool_completion", "test passed");
}

#[tokio::test]
async fn a2_single_tool_then_answer() {
    info!(target: "borg_agent_test", test = "a2_single_tool_then_answer", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "search and answer".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![
        Ok(assistant_tool_calls(vec![(
            "tc1",
            "search",
            json!({"query":"x"}),
        )])),
        Ok(assistant_text("done")),
    ]);
    let (runner, mut calls_rx) = scripted_runner(vec![Ok(ToolResponse {
        content: ToolResultData::Text("hits: []".to_string()),
    })]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    assert!(calls_rx.try_recv().is_ok());
    info!(target: "borg_agent_test", test = "a2_single_tool_then_answer", "test passed");
}

#[tokio::test]
async fn a3_multiple_tools_keep_order() {
    info!(target: "borg_agent_test", test = "a3_multiple_tools_keep_order", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "run two tools".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![
        Ok(assistant_tool_calls(vec![
            ("tc1", "search", json!({"query":"one"})),
            ("tc2", "execute", json!({"code":"1+1"})),
        ])),
        Ok(assistant_text("done")),
    ]);
    let (runner, mut calls_rx) = scripted_runner(vec![
        Ok(ToolResponse {
            content: ToolResultData::Text("a=1".to_string()),
        }),
        Ok(ToolResponse {
            content: ToolResultData::Text("b=2".to_string()),
        }),
    ]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let _ = agent.run(&mut session, &provider, &tools).await;
    let first = calls_rx.try_recv().unwrap();
    let second = calls_rx.try_recv().unwrap();
    assert_eq!(first.tool_name, "search");
    assert_eq!(second.tool_name, "execute");
    debug!(
        target: "borg_agent_test",
        first_tool = first.tool_name.as_str(),
        second_tool = second.tool_name.as_str(),
        "validated tool call ordering"
    );
}

#[tokio::test]
async fn a5_follow_up_continues_run() {
    info!(target: "borg_agent_test", test = "a5_follow_up_continues_run", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "first".to_string(),
        })
        .await
        .unwrap();
    session.enqueue_follow_up_message(Message::User {
        content: "follow-up".to_string(),
    });

    let (provider, _requests_rx) = scripted_provider(vec![
        Ok(assistant_text("turn-1")),
        Ok(assistant_text("turn-2")),
    ]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    let msgs = session.read_messages(0, 256).await.unwrap();
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Message::User { content } if content == "follow-up"))
    );
    info!(target: "borg_agent_test", test = "a5_follow_up_continues_run", "test passed");
}

#[tokio::test]
async fn a6_tool_error_is_returned_as_tool_result() {
    info!(target: "borg_agent_test", test = "a6_tool_error_is_returned_as_tool_result", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "tool error path".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![
        Ok(assistant_tool_calls(vec![(
            "tc1",
            "execute",
            json!({"code":"bad"}),
        )])),
        Ok(assistant_text("handled")),
    ]);
    let (runner, _calls_rx) = scripted_runner(vec![Err("execution failed".to_string())]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    let msgs = session.read_messages(0, 256).await.unwrap();
    assert!(msgs.iter().any(|m| {
        matches!(
            m,
            Message::ToolResult {
                content: ToolResultData::Error { .. },
                ..
            }
        )
    }));
    info!(target: "borg_agent_test", test = "a6_tool_error_is_returned_as_tool_result", "test passed");
}

#[tokio::test]
async fn a7_provider_failure_surfaces_session_error() {
    info!(target: "borg_agent_test", test = "a7_provider_failure_surfaces_session_error", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "provider fail".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![Err("provider down".to_string())]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::SessionError(_)));
    info!(target: "borg_agent_test", test = "a7_provider_failure_surfaces_session_error", "test passed");
}

#[tokio::test]
async fn a8_idle_run_when_no_new_messages() {
    info!(target: "borg_agent_test", test = "a8_idle_run_when_no_new_messages", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    let (provider, _requests_rx) = scripted_provider(vec![]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Idle));
    info!(target: "borg_agent_test", test = "a8_idle_run_when_no_new_messages", "test passed");
}

#[tokio::test]
async fn a9_lifecycle_events_persisted_once() {
    info!(target: "borg_agent_test", test = "a9_lifecycle_events_persisted_once", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    session
        .add_message(Message::User {
            content: "hello".to_string(),
        })
        .await
        .unwrap();
    let (provider, _requests_rx) = scripted_provider(vec![Ok(assistant_text("ok"))]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let _ = agent.run(&mut session, &provider, &tools).await;
    let messages = session.read_messages(0, 256).await.unwrap();
    let started = messages
        .iter()
        .filter(|m| matches!(m, Message::SessionEvent { name, .. } if name == "agent_started"))
        .count();
    let finished = messages
        .iter()
        .filter(|m| matches!(m, Message::SessionEvent { name, .. } if name == "agent_finished"))
        .count();
    assert_eq!(started, 1);
    assert_eq!(finished, 1);
    debug!(
        target: "borg_agent_test",
        started,
        finished,
        "validated lifecycle event cardinality"
    );
}

#[tokio::test]
async fn injected_tool_runner_helper_still_works() {
    init_test_tracing();
    info!(target: "borg_agent_test", test = "injected_tool_runner_helper_still_works", "starting test");
    struct InlineRunner;
    #[async_trait]
    impl ToolRunner for InlineRunner {
        async fn run(&self, _request: ToolRequest) -> Result<ToolResponse> {
            Ok(ToolResponse {
                content: ToolResultData::Text("ok".to_string()),
            })
        }
    }

    let tools = AgentTools {
        tool_runner: &InlineRunner,
    };
    let out = call_tool(&tools, "tc1", "search", &json!({"query": "x"}))
        .await
        .unwrap();
    assert!(matches!(out, ToolResultData::Text(text) if text == "ok"));
    info!(target: "borg_agent_test", test = "injected_tool_runner_helper_still_works", "test passed");
}

#[tokio::test]
async fn toolchain_rejects_invalid_input_shape() {
    let toolchain = Toolchain::builder()
        .add_tool(Tool::new(
            ToolSpec {
                name: "search".to_string(),
                description: "search".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            },
            None,
            |_request| async move {
                Ok(ToolResponse {
                    content: ToolResultData::Text("ok".to_string()),
                })
            },
        ))
        .unwrap()
        .build()
        .unwrap();
    let tools = AgentTools {
        tool_runner: &toolchain,
    };

    let err = call_tool(&tools, "tc1", "search", &json!({}))
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("missing required property `query`")
    );
}

#[tokio::test]
async fn toolchain_validates_output_shape_when_configured() {
    let toolchain = Toolchain::builder()
        .add_tool(Tool::new(
            ToolSpec {
                name: "execute".to_string(),
                description: "execute".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "code": { "type": "string" } },
                    "required": ["code"],
                    "additionalProperties": false
                }),
            },
            Some(json!({
                "type": "object",
                "properties": { "Text": { "type": "string" } },
                "required": ["Text"],
                "additionalProperties": false
            })),
            |_request| async move {
                Ok(ToolResponse {
                    content: ToolResultData::Error {
                        message: "mismatch".to_string(),
                    },
                })
            },
        ))
        .unwrap()
        .build()
        .unwrap();
    let tools = AgentTools {
        tool_runner: &toolchain,
    };

    let err = call_tool(
        &tools,
        "tc2",
        "execute",
        &json!({"code":"async () => { return 1; }"}),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("missing required property `Text`"));
}

#[test]
fn llm_adapter_rejects_orphan_tool_results() {
    let messages = vec![
        Message::System {
            content: "system".to_string(),
        },
        Message::ToolResult {
            tool_call_id: "call_orphan".to_string(),
            name: "execute".to_string(),
            content: ToolResultData::Text("orphan".to_string()),
        },
        Message::ToolCall {
            tool_call_id: "call_ok".to_string(),
            name: "search".to_string(),
            arguments: json!({"query":"x"}),
        },
        Message::ToolResult {
            tool_call_id: "call_ok".to_string(),
            name: "search".to_string(),
            content: ToolResultData::Text("ok".to_string()),
        },
    ];

    let err = to_provider_messages(&messages).unwrap_err();
    assert!(
        err.to_string().contains("invalid tool message ordering")
            || err.to_string().contains("orphan tool result detected")
    );
}

#[test]
fn llm_adapter_rejects_non_adjacent_tool_result() {
    let messages = vec![
        Message::ToolCall {
            tool_call_id: "call_1".to_string(),
            name: "search".to_string(),
            arguments: json!({"query":"x"}),
        },
        Message::Assistant {
            content: "interleaving assistant text".to_string(),
        },
        Message::ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "search".to_string(),
            content: ToolResultData::Text("ok".to_string()),
        },
    ];

    let err = to_provider_messages(&messages).unwrap_err();
    assert!(
        err.to_string().contains("invalid tool message ordering")
            || err.to_string().contains("orphan tool result detected")
    );
}

#[test]
fn llm_adapter_auto_closes_dangling_tool_call_before_user_message() {
    let messages = vec![
        Message::System {
            content: "system".to_string(),
        },
        Message::ToolCall {
            tool_call_id: "call_1".to_string(),
            name: "execute".to_string(),
            arguments: json!({"code":"async () => { return 1; }"}),
        },
        Message::User {
            content: "continue".to_string(),
        },
    ];

    let provider_messages = to_provider_messages(&messages).unwrap();
    assert_eq!(provider_messages.len(), 4);

    assert!(matches!(
        provider_messages.get(1),
        Some(borg_llm::ProviderMessage::Assistant { content })
            if matches!(content.first(), Some(borg_llm::ProviderBlock::ToolCall { id, .. }) if id == "call_1")
    ));
    assert!(matches!(
        provider_messages.get(2),
        Some(borg_llm::ProviderMessage::ToolResult {
            tool_call_id,
            name,
            content
        })
            if tool_call_id == "call_1"
                && name == "execute"
                && matches!(content.first(), Some(borg_llm::ProviderBlock::Text(text)) if text.contains("tool execution interrupted"))
    ));
}
