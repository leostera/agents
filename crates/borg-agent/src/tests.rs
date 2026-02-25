
use std::path::PathBuf;

use super::{
    Agent, AgentTools, Message, Session, SessionResult, ToolRequest, ToolResponse, ToolResultData,
    ToolRunner, call_tool,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_db::BorgDb;
use borg_llm::{LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, StopReason};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

struct ScriptedRunner {
    calls_tx: mpsc::UnboundedSender<ToolRequest>,
    outputs_rx: Mutex<mpsc::UnboundedReceiver<Result<ToolResponse, String>>>,
}

#[async_trait]
impl ToolRunner for ScriptedRunner {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
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

async fn make_test_db() -> Result<BorgDb> {
    let path = PathBuf::from(format!("/tmp/borg-agent-test-{}.db", Uuid::now_v7()));
    let borg_db = BorgDb::open_local(path.to_string_lossy().as_ref()).await?;
    borg_db.migrate().await?;
    Ok(borg_db)
}

async fn make_session() -> Result<(Agent, Session)> {
    let db = make_test_db().await?;
    let agent = Agent::new("test-agent").with_system_prompt("system prompt");
    let session = Session::new("test-session", agent.clone(), db).await?;
    Ok((agent, session))
}

#[tokio::test]
async fn a1_no_tool_completion() {
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
}

#[tokio::test]
async fn a2_single_tool_then_answer() {
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
}

#[tokio::test]
async fn a3_multiple_tools_keep_order() {
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
}

#[tokio::test]
async fn a5_follow_up_continues_run() {
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
}

#[tokio::test]
async fn a6_tool_error_is_returned_as_tool_result() {
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
}

#[tokio::test]
async fn a7_provider_failure_surfaces_session_error() {
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
}

#[tokio::test]
async fn a8_idle_run_when_no_new_messages() {
    let (agent, mut session) = make_session().await.unwrap();
    let (provider, _requests_rx) = scripted_provider(vec![]);
    let (runner, _calls_rx) = scripted_runner(vec![]);
    let tools = AgentTools {
        tool_runner: &runner,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
    assert!(matches!(result, SessionResult::Idle));
}

#[tokio::test]
async fn a9_lifecycle_events_persisted_once() {
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
}

#[tokio::test]
async fn injected_tool_runner_helper_still_works() {
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
}
