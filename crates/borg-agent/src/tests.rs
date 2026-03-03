use std::path::PathBuf;
use std::sync::Arc;

use super::{
    Agent, ContextChunk, ContextManager, ContextManagerStrategy, Message, Session, SessionResult,
    StaticContextProvider, Tool, ToolRequest, ToolResponse, ToolResultData, ToolSpec, Toolchain,
    to_provider_messages,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::uri;
use borg_db::BorgDb;
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, StopReason, TranscriptionRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Once;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, trace};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

struct ScriptedProvider {
    requests_tx: mpsc::UnboundedSender<LlmRequest>,
    responses_rx: Mutex<mpsc::UnboundedReceiver<Result<LlmAssistantMessage, String>>>,
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn chat(&self, req: &LlmRequest) -> borg_llm::Result<LlmAssistantMessage> {
        trace!(
            target: "borg_agent_test",
            model = req.model.as_str(),
            message_count = req.messages.len(),
            "scripted provider received chat request"
        );
        self.requests_tx
            .send(req.clone())
            .map_err(|e| borg_llm::LlmError::message(e.to_string()))?;
        self.responses_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| borg_llm::LlmError::message("missing scripted llm response"))?
            .map_err(borg_llm::LlmError::message)
    }

    async fn transcribe(&self, _req: &TranscriptionRequest) -> borg_llm::Result<String> {
        Err(borg_llm::LlmError::message(
            "transcribe not supported in scripted provider",
        ))
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

fn scripted_toolchain(
    outputs: Vec<Result<ToolResponse, String>>,
) -> Result<(Toolchain, mpsc::UnboundedReceiver<ToolRequest>)> {
    let (calls_tx, calls_rx) = mpsc::unbounded_channel();
    let (outputs_tx, outputs_rx) = mpsc::unbounded_channel();
    for output in outputs {
        outputs_tx.send(output)?;
    }
    drop(outputs_tx);
    let outputs_rx = Arc::new(Mutex::new(outputs_rx));
    let mut toolchain = Toolchain::new();
    for tool_name in ["search", "execute"] {
        let calls_tx = calls_tx.clone();
        let outputs_rx = Arc::clone(&outputs_rx);
        toolchain.register(Tool::new(
            ToolSpec {
                name: tool_name.to_string(),
                description: format!("scripted {}", tool_name),
                parameters: json!({ "type": "object", "additionalProperties": true }),
            },
            None,
            move |request| {
                let calls_tx = calls_tx.clone();
                let outputs_rx = Arc::clone(&outputs_rx);
                async move {
                    trace!(
                        target: "borg_agent_test",
                        tool_call_id = request.tool_call_id.as_str(),
                        tool_name = request.tool_name.as_str(),
                        "scripted toolchain received tool request"
                    );
                    calls_tx.send(request).map_err(|e| anyhow!(e.to_string()))?;
                    outputs_rx
                        .lock()
                        .await
                        .recv()
                        .await
                        .ok_or_else(|| anyhow!("missing scripted tool output"))?
                        .map_err(|e| anyhow!(e))
                }
            },
        ))?;
    }
    Ok((toolchain, calls_rx))
}

fn assistant_text(text: &str) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: vec![ProviderBlock::Text(text.to_string())],
        stop_reason: StopReason::EndOfTurn,
        error_message: None,
        usage_tokens: None,
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
        usage_tokens: None,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct EchoArgs {
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct EchoResult {
    echoed: String,
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
    let tools = Toolchain::new();

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
    let (tools, mut calls_rx) = scripted_toolchain(vec![Ok(ToolResponse {
        content: ToolResultData::Text("hits: []".to_string()),
    })])
    .unwrap();

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
    let (tools, mut calls_rx) = scripted_toolchain(vec![
        Ok(ToolResponse {
            content: ToolResultData::Text("a=1".to_string()),
        }),
        Ok(ToolResponse {
            content: ToolResultData::Text("b=2".to_string()),
        }),
    ])
    .unwrap();

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
async fn a17_typed_toolchain_decodes_provider_arguments() {
    let db = make_test_db().await.unwrap();
    let agent = Agent::<EchoArgs, EchoResult>::new(uri!("borg", "agent", "typed-agent"))
        .with_system_prompt("system prompt");
    let mut session = Session::<EchoArgs, EchoResult>::new(
        uri!("borg", "session", "typed-session"),
        agent.clone(),
        db,
    )
    .await
    .unwrap();
    session
        .add_message(Message::User {
            content: "echo hello".to_string(),
        })
        .await
        .unwrap();

    let (provider, _requests_rx) = scripted_provider(vec![
        Ok(assistant_tool_calls(vec![(
            "tc-typed-1",
            "echo",
            json!({"text":"hello"}),
        )])),
        Ok(assistant_text("done")),
    ]);

    let (calls_tx, mut calls_rx) = mpsc::unbounded_channel::<ToolRequest<EchoArgs>>();
    let toolchain = Toolchain::<EchoArgs, EchoResult>::builder()
        .add_tool(
            Tool::new_typed(
                ToolSpec {
                    name: "echo".to_string(),
                    description: "echo typed args".to_string(),
                    parameters: json!({
                        "type":"object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }),
                },
                None,
                move |request| {
                    let calls_tx = calls_tx.clone();
                    async move {
                        calls_tx.send(request.clone()).unwrap();
                        Ok(ToolResponse {
                            content: ToolResultData::Execution {
                                result: EchoResult {
                                    echoed: request.arguments.text,
                                },
                                duration: Duration::from_millis(1),
                            },
                        })
                    }
                },
            ),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = agent.run(&mut session, &provider, &toolchain).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    let call = calls_rx.try_recv().unwrap();
    assert_eq!(
        call.arguments,
        EchoArgs {
            text: "hello".to_string()
        }
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
    let tools = Toolchain::new();

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
    let (tools, _calls_rx) = scripted_toolchain(vec![Err("execution failed".to_string())]).unwrap();

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
    let tools = Toolchain::new();

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::SessionError(_)));
    info!(target: "borg_agent_test", test = "a7_provider_failure_surfaces_session_error", "test passed");
}

#[tokio::test]
async fn a8_idle_run_when_no_new_messages() {
    info!(target: "borg_agent_test", test = "a8_idle_run_when_no_new_messages", "starting test");
    let (agent, mut session) = make_session().await.unwrap();
    let (provider, _requests_rx) = scripted_provider(vec![]);
    let tools = Toolchain::new();

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
    let tools = Toolchain::new();

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
async fn injected_toolchain_helper_still_works() {
    init_test_tracing();
    info!(target: "borg_agent_test", test = "injected_toolchain_helper_still_works", "starting test");
    let tools = Toolchain::builder()
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
    let out = tools
        .run(ToolRequest {
            tool_call_id: "tc1".to_string(),
            tool_name: "search".to_string(),
            arguments: json!({"query": "x"}),
        })
        .await
        .unwrap()
        .content;
    assert!(matches!(out, ToolResultData::Text(text) if text == "ok"));
    info!(target: "borg_agent_test", test = "injected_toolchain_helper_still_works", "test passed");
}

#[tokio::test]
async fn toolchain_delegates_input_validation_to_tool_callback() {
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
    let out = toolchain
        .run(ToolRequest {
            tool_call_id: "tc1".to_string(),
            tool_name: "search".to_string(),
            arguments: json!({}),
        })
        .await
        .unwrap()
        .content;
    assert!(matches!(out, ToolResultData::Text(text) if text == "ok"));
}

#[tokio::test]
async fn toolchain_does_not_validate_output_schema() {
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
    let out = toolchain
        .run(ToolRequest {
            tool_call_id: "tc2".to_string(),
            tool_name: "execute".to_string(),
            arguments: json!({"code":"async () => { return 1; }"}),
        })
        .await
        .unwrap()
        .content;
    assert!(matches!(
        out,
        ToolResultData::Error { message } if message == "mismatch"
    ));
}

#[test]
fn llm_adapter_rejects_orphan_tool_results() {
    let messages: Vec<Message<Value, Value>> = vec![
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
    let messages: Vec<Message<Value, Value>> = vec![
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
    let messages: Vec<Message<Value, Value>> = vec![
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

#[tokio::test]
async fn context_window_splits_messages_into_typed_sections() {
    let agent = Agent::<Value, Value>::new(uri!("borg", "agent", "context-sections"))
        .with_system_prompt("system-prompt")
        .with_behavior_prompt("behavior-prompt")
        .with_tools(vec![ToolSpec {
            name: "Apps-getApp".to_string(),
            description: "Get app details".to_string(),
            parameters: json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
        }]);
    let messages = vec![
        Message::User {
            content: "hi".to_string(),
        },
        Message::Assistant {
            content: "hello".to_string(),
        },
        Message::ToolCall {
            tool_call_id: "call_1".to_string(),
            name: "Apps-getApp".to_string(),
            arguments: json!({"id":"borg:app:github"}),
        },
        Message::ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "Apps-getApp".to_string(),
            content: ToolResultData::Text("{\"app\":{}}".to_string()),
        },
    ];

    let manager = ContextManager::builder()
        .with_strategy(ContextManagerStrategy::Passthrough)
        .build();
    let context = manager.build_context(&agent, &messages).await.unwrap();

    assert_eq!(context.system_prompt, "system-prompt");
    assert_eq!(context.behavior_prompt, "behavior-prompt");
    assert_eq!(context.available_tools.len(), 1);
    assert_eq!(context.available_capabilities.len(), 1);
    assert_eq!(context.available_capabilities[0].name, "Apps-getApp");
    assert_eq!(context.user_messages.len(), 1);
    assert_eq!(context.assistant_messages.len(), 1);
    assert_eq!(context.tool_calls.len(), 1);
    assert_eq!(context.tool_responses.len(), 1);
}

#[tokio::test]
async fn context_provider_input_messages_start_with_system_then_behavior() {
    let agent = Agent::<Value, Value>::new(uri!("borg", "agent", "context-order"))
        .with_system_prompt("system-prompt")
        .with_behavior_prompt("behavior-prompt")
        .with_tools(vec![]);
    let messages = vec![Message::User {
        content: "hi".to_string(),
    }];

    let manager = ContextManager::builder()
        .with_strategy(ContextManagerStrategy::Passthrough)
        .build();
    let context = manager.build_context(&agent, &messages).await.unwrap();
    let provider_messages = context.provider_input_messages();

    assert!(matches!(
        provider_messages.first(),
        Some(Message::System { content }) if content == "system-prompt"
    ));
    assert!(matches!(
        provider_messages.get(1),
        Some(Message::System { content }) if content == "behavior-prompt"
    ));
}

#[tokio::test]
async fn compaction_keeps_prompts_tools_and_capabilities_uncompacted() {
    let agent = Agent::<Value, Value>::new(uri!("borg", "agent", "context-compact"))
        .with_system_prompt("system-prompt")
        .with_behavior_prompt("behavior-prompt")
        .with_tools(vec![ToolSpec {
            name: "Apps-listApps".to_string(),
            description: "List apps".to_string(),
            parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
        }]);
    let messages = vec![
        Message::User {
            content: "this is a very long user message that should trigger compaction 1"
                .to_string(),
        },
        Message::Assistant {
            content: "this is a very long assistant message that should trigger compaction 2"
                .to_string(),
        },
        Message::User {
            content: "this is a very long user message that should trigger compaction 3"
                .to_string(),
        },
    ];

    let manager = ContextManager::builder()
        .with_strategy(ContextManagerStrategy::Compacting {
            max_chars: 60,
            keep_recent_messages: 1,
        })
        .build();
    let context = manager.build_context(&agent, &messages).await.unwrap();
    let provider_messages = context.provider_input_messages();

    assert_eq!(context.system_prompt, "system-prompt");
    assert_eq!(context.behavior_prompt, "behavior-prompt");
    assert_eq!(context.available_tools.len(), 1);
    assert_eq!(context.available_capabilities.len(), 1);
    assert!(provider_messages.iter().any(|message| {
        matches!(
            message,
            Message::System { content } if content.contains("Compacted conversation summary")
        )
    }));
}

#[tokio::test]
async fn context_filters_persisted_prompt_messages_from_ordered_history() {
    let agent = Agent::<Value, Value>::new(uri!("borg", "agent", "context-dedup"))
        .with_system_prompt("system-prompt")
        .with_behavior_prompt("behavior-prompt")
        .with_tools(vec![]);
    let messages = vec![
        Message::System {
            content: "system-prompt".to_string(),
        },
        Message::System {
            content: "behavior-prompt".to_string(),
        },
        Message::User {
            content: "hello".to_string(),
        },
    ];

    let manager = ContextManager::builder()
        .with_strategy(ContextManagerStrategy::Passthrough)
        .build();
    let context = manager.build_context(&agent, &messages).await.unwrap();
    let provider_messages = context.provider_input_messages();
    let prompt_count = provider_messages
        .iter()
        .filter(|message| {
            matches!(
                message,
                Message::System { content } if content == "system-prompt" || content == "behavior-prompt"
            )
        })
        .count();

    assert_eq!(prompt_count, 2);
}

#[tokio::test]
async fn context_manager_keeps_pinned_provider_chunks_uncompacted() {
    let agent = Agent::<Value, Value>::new(uri!("borg", "agent", "context-pinned-provider"))
        .with_system_prompt("system-prompt")
        .with_behavior_prompt("behavior-prompt")
        .with_tools(vec![]);
    let messages = vec![
        Message::User {
            content: "this user message is long enough to encourage compaction and summarization"
                .to_string(),
        },
        Message::Assistant {
            content:
                "this assistant message is also long enough to force compaction in this test case"
                    .to_string(),
        },
        Message::User {
            content: "keep only this one as recent".to_string(),
        },
    ];

    let manager = ContextManager::builder()
        .with_strategy(ContextManagerStrategy::Compacting {
            max_chars: 60,
            keep_recent_messages: 1,
        })
        .add_provider(StaticContextProvider::new(vec![ContextChunk::pinned(
            vec![Message::System {
                content: "PINNED_PROVIDER_CONTEXT".to_string(),
            }],
        )]))
        .build();
    let context = manager.build_context(&agent, &messages).await.unwrap();
    let provider_messages = context.provider_input_messages();

    assert!(provider_messages.iter().any(|message| {
        matches!(
            message,
            Message::System { content } if content == "PINNED_PROVIDER_CONTEXT"
        )
    }));
    assert!(provider_messages.iter().any(|message| {
        matches!(
            message,
            Message::System { content } if content.contains("Compacted conversation summary")
        )
    }));
}
