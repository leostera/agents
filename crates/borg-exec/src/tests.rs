use std::path::PathBuf;
use std::sync::Once;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use borg_agent::{Agent, Message, Session, SessionResult, ToolRequest, ToolResultData};
use borg_apps::default_tool_specs as default_apps_tool_specs;
use borg_codemode::{
    CodeModeContext, CodeModeRuntime, default_tool_specs as default_codemode_tool_specs,
};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_fs::{BorgFs, FileKind, PutFileMetadata};
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    TranscriptionRequest,
};
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tracing_subscriber::EnvFilter;

use crate::BorgSupervisor;
use crate::mailbox_envelope::ActorMailboxEnvelope;
use crate::session_manager::SessionManager;
use crate::tool_runner::build_exec_toolchain_with_context;
use crate::tool_runner::default_exec_admin_tool_specs;

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("borg-exec-test-{}.db", uuid::Uuid::now_v7()))
}

fn temp_memory_paths() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("borg-exec-ltm-{}", uuid::Uuid::now_v7()));
    let search = std::env::temp_dir().join(format!("borg-exec-search-{}", uuid::Uuid::now_v7()));
    (root, search)
}

fn temp_files_root() -> PathBuf {
    std::env::temp_dir().join(format!("borg-fs-test-{}", uuid::Uuid::now_v7()))
}

async fn open_test_memory() -> MemoryStore {
    let (ltm_root, search_root) = temp_memory_paths();
    let memory = MemoryStore::new(&ltm_root, &search_root).unwrap();
    memory.migrate().await.unwrap();
    memory
}

async fn open_test_db() -> BorgDb {
    let path = temp_db_path();
    let db = BorgDb::open_local(path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();
    db
}

fn init_test_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("info,borg_exec=debug,borg_db=debug,borg_llm=debug")
            }))
            .with_test_writer()
            .try_init()
            .ok();
    });
}

#[derive(Clone)]
struct ScriptedProvider {
    requests: Arc<StdMutex<Vec<LlmRequest>>>,
    responses_rx:
        Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Result<LlmAssistantMessage, String>>>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<Result<LlmAssistantMessage, String>>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        for response in responses {
            tx.send(response).expect("seed provider response queue");
        }
        Self {
            requests: Arc::new(StdMutex::new(Vec::new())),
            responses_rx: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    fn requests(&self) -> Vec<LlmRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn chat(&self, req: &LlmRequest) -> borg_llm::Result<LlmAssistantMessage> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(req.clone());
        self.responses_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| borg_llm::LlmError::message("scripted provider exhausted"))?
            .map_err(borg_llm::LlmError::message)
    }

    async fn transcribe(&self, _req: &TranscriptionRequest) -> borg_llm::Result<String> {
        Err(borg_llm::LlmError::message(
            "transcribe not supported in scripted provider",
        ))
    }
}

fn assistant_text(text: &str) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: vec![ProviderBlock::Text(text.to_string())],
        stop_reason: StopReason::EndOfTurn,
        error_message: None,
        usage_tokens: None,
    }
}

fn assistant_tool_call(
    tool_call_id: &str,
    name: &str,
    args: borg_agent::BorgToolCall,
) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: vec![ProviderBlock::ToolCall {
            id: tool_call_id.to_string(),
            name: name.to_string(),
            arguments_json: args.to_value().expect("valid tool call arguments"),
        }],
        stop_reason: StopReason::ToolCall,
        error_message: None,
        usage_tokens: None,
    }
}

#[derive(Debug, Deserialize)]
struct CodeModeEnvResult {
    keys: Vec<String>,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActorsSendMessageResult {
    status: String,
    actor_message_id: String,
    submission_id: String,
}

#[derive(Debug, Deserialize)]
struct ActorsReceiveResult {
    status: String,
    actor_message_id: String,
    submission_id: String,
    in_reply_to_submission_id: Option<String>,
    source_actor_id: Option<String>,
    text: String,
}

fn default_agent_tools() -> Vec<borg_agent::ToolSpec> {
    let mut tools = default_codemode_tool_specs();
    tools.extend(default_apps_tool_specs());
    tools
}

#[test]
fn default_exec_admin_specs_include_borgfs_tools() {
    let tools = default_exec_admin_tool_specs();
    assert!(tools.iter().any(|tool| tool.name == "BorgFS-ls"));
    assert!(tools.iter().any(|tool| tool.name == "BorgFS-put"));
    assert!(tools.iter().any(|tool| tool.name == "BorgFS-settings"));
}

#[tokio::test]
async fn session_manager_resolve_agent_for_turn_refreshes_prompts_and_tools_each_call() {
    let db = open_test_db().await;
    let manager = SessionManager::new(db.clone());
    let actor_id = uri!("borg", "actor", "refresh-test");

    db.upsert_actor(&actor_id, "refresh-test", "prompt-v1", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();

    let first = manager.resolve_agent_for_turn(&actor_id).await.unwrap();
    assert_eq!(first.system_prompt, "prompt-v1");
    assert!(
        first
            .tools
            .iter()
            .any(|tool| tool.name == "ShellMode-executeCommand")
    );
    assert!(
        first
            .tools
            .iter()
            .any(|tool| tool.name == "CodeMode-searchApis")
    );
    assert!(first.tools.iter().any(|tool| tool.name == "Memory-search"));
    assert!(
        first
            .tools
            .iter()
            .any(|tool| tool.name == "TaskGraph-createTask")
    );
    assert!(
        first
            .tools
            .iter()
            .any(|tool| tool.name == "Schedule-createJob")
    );
    assert!(
        !first
            .tools
            .iter()
            .any(|tool| tool.name == "CustomApp-doThing")
    );

    let app_id = uri!("borg", "app", "refresh-tools");
    let capability_id = uri!("borg", "capability", "refresh-tools-do-thing");
    db.upsert_app(
        &app_id,
        "Refresh Tools App",
        "refresh-tools",
        "test app",
        "active",
    )
    .await
    .unwrap();
    db.upsert_app_capability(
        &app_id,
        &capability_id,
        "CustomApp-doThing",
        "Do a thing",
        "codemode",
        "Run this when needed.",
        "active",
    )
    .await
    .unwrap();
    db.upsert_actor(&actor_id, "refresh-test", "prompt-v2", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();

    let second = manager.resolve_agent_for_turn(&actor_id).await.unwrap();
    assert_eq!(second.system_prompt, "prompt-v2");
    assert!(
        second
            .tools
            .iter()
            .any(|tool| tool.name == "ShellMode-executeCommand")
    );
    assert!(
        second
            .tools
            .iter()
            .any(|tool| tool.name == "CustomApp-doThing")
    );
}

#[tokio::test]
async fn e2e_agent_toolchain_runtime_search_then_execute_then_reply() {
    init_test_tracing();
    let db = open_test_db().await;
    let session_db = db.clone();
    let agent = Agent::new(uri!("borg", "agent", "exec-e2e"))
        .with_system_prompt("Use tools when needed and provide a final concise answer.")
        .with_tools(default_agent_tools());
    let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
        .await
        .unwrap();
    session
        .add_message(Message::User {
            content: "List APIs, then run code to inspect working directory entries.".to_string(),
        })
        .await
        .unwrap();

    let provider = ScriptedProvider::new(vec![
        Ok(assistant_tool_call(
            "call_search_1",
            "CodeMode-searchApis",
            json!({ "query": "ls fetch" }).into(),
        )),
        Ok(assistant_tool_call(
            "call_exec_1",
            "CodeMode-executeCode",
            json!({
                "hint": "Inspecting working directory entries",
                "code": "async () => { const listing = Borg.OS.ls('.'); return { has_entries: listing.entries.length > 0, first_entry: listing.entries[0] ?? null }; }"
            })
            .into(),
        )),
        Ok(assistant_text(
            "Completed runtime plan. BORG_EXEC_TOOLCHAIN_RT_OK",
        )),
    ]);
    let toolchain_db = open_test_db().await;
    let toolchain_fs = BorgFs::local(toolchain_db.clone(), temp_files_root());

    let toolchain = build_exec_toolchain_with_context(
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        CodeModeContext::default(),
        open_test_memory().await,
        toolchain_db,
        toolchain_fs,
        uri!("borg", "session", "test-runtime"),
        uri!("borg", "agent", "test-runtime"),
        uri!("borg", "user", "test-runtime"),
        true,
    )
    .unwrap();
    let result = agent.run(&mut session, &provider, &toolchain).await;
    let output = match result {
        SessionResult::Completed(Ok(output)) => output,
        other => panic!("unexpected session result: {:?}", other),
    };
    assert!(output.reply.contains("BORG_EXEC_TOOLCHAIN_RT_OK"));

    let messages = session.read_messages(0, 256).await.unwrap();
    assert!(messages.iter().any(
        |message| matches!(message, Message::ToolCall { name, .. } if name == "CodeMode-searchApis")
    ));
    assert!(
        messages.iter().any(
            |message| matches!(message, Message::ToolCall { name, .. } if name == "CodeMode-executeCode")
        )
    );
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            Message::ToolResult {
                content: ToolResultData::Text(text),
                ..
            } if text.contains("interface BorgSdk")
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            Message::ToolResult {
                content: ToolResultData::Execution { result, .. },
                ..
            } if result
                .to_value()
                .ok()
                .and_then(|value| value.get("has_entries").cloned())
                .is_some()
        )
    }));

    let requests = provider.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].messages.iter().any(|message| {
        matches!(
            message,
            ProviderMessage::ToolResult { name, .. } if name == "CodeMode-searchApis"
        )
    }));
    assert!(requests[2].messages.iter().any(|message| {
        matches!(
            message,
            ProviderMessage::ToolResult { name, .. } if name == "CodeMode-executeCode"
        )
    }));

    let tool_calls = session_db.list_tool_calls(20).await.unwrap();
    assert!(tool_calls.len() >= 2);
    assert!(
        tool_calls
            .iter()
            .any(|record| record.tool_name == "CodeMode-searchApis")
    );
    assert!(
        tool_calls
            .iter()
            .any(|record| record.tool_name == "CodeMode-executeCode")
    );
}

#[tokio::test]
async fn e2e_agent_toolchain_runtime_invalid_execute_returns_tool_error_and_recovers() {
    init_test_tracing();
    let db = open_test_db().await;
    let agent = Agent::new(uri!("borg", "agent", "exec-e2e-invalid"))
        .with_system_prompt("Call CodeMode-executeCode and then summarize the outcome.")
        .with_tools(default_agent_tools());
    let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
        .await
        .unwrap();
    session
        .add_message(Message::User {
            content: "Run execute with code.".to_string(),
        })
        .await
        .unwrap();

    let provider = ScriptedProvider::new(vec![
        Ok(assistant_tool_call(
            "call_exec_bad",
            "CodeMode-executeCode",
            json!({
                "hint": "Running invalid execute payload for error handling test",
                "code": "Borg.OS.ls('.')"
            })
            .into(),
        )),
        Ok(assistant_text("Saw tool failure and handled it.")),
    ]);
    let toolchain_db = open_test_db().await;
    let toolchain_fs = BorgFs::local(toolchain_db.clone(), temp_files_root());

    let toolchain = build_exec_toolchain_with_context(
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        CodeModeContext::default(),
        open_test_memory().await,
        toolchain_db,
        toolchain_fs,
        uri!("borg", "session", "test-invalid"),
        uri!("borg", "agent", "test-invalid"),
        uri!("borg", "user", "test-invalid"),
        true,
    )
    .unwrap();
    let result = agent.run(&mut session, &provider, &toolchain).await;
    assert!(matches!(result, SessionResult::Completed(Ok(_))));

    let messages = session.read_messages(0, 256).await.unwrap();
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            Message::ToolResult {
                content: ToolResultData::Error { message },
                ..
            } if message.contains("async zero-arg function expression")
        )
    }));
}

#[tokio::test]
async fn app_available_secret_is_exposed_in_borg_env_get() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );

    let app_id = uri!("borg", "app", "github-env-test");
    let connection_id = uri!("borg", "app-connection", "github-env-test");
    let owner_user_id = uri!("borg", "user", "env-owner");
    let secret_id = uri!("borg", "app-secret", "github-env-test-token");

    db.upsert_app_with_metadata(
        &app_id,
        "GitHub Test",
        "github-test",
        "test app for env secret exposure",
        "active",
        false,
        "custom",
        "none",
        &json!({}),
        &["GITHUB_ACCESS_TOKEN".to_string()],
    )
    .await
    .unwrap();

    db.upsert_app_connection(
        &app_id,
        &connection_id,
        Some(&owner_user_id),
        None,
        None,
        "connected",
        &json!({}),
    )
    .await
    .unwrap();

    // Stored key remains oauth-native (`access_token`), while exposed env key is `GITHUB_ACCESS_TOKEN`.
    db.upsert_app_secret(
        &app_id,
        &secret_id,
        Some(&connection_id),
        "access_token",
        "gho_test_123",
        "oauth",
    )
    .await
    .unwrap();

    let toolchain = runtime
        .build_toolchain(
            &owner_user_id,
            &uri!("borg", "session", "env-test"),
            &uri!("borg", "agent", "env-test"),
        )
        .await
        .unwrap();

    let response = toolchain
        .run(ToolRequest {
            tool_call_id: "tool-call-env-1".to_string(),
            tool_name: "CodeMode-executeCode".to_string(),
            arguments: json!({
                "hint": "verify app secret projection into env",
                "code": "async () => ({ keys: Borg.env.keys(), token: Borg.env.get('GITHUB_ACCESS_TOKEN') })"
            })
            .into(),
        })
        .await
        .unwrap();

    match response.content {
        ToolResultData::Execution { result, .. } => {
            let result: CodeModeEnvResult =
                serde_json::from_value(result.to_value().expect("result value"))
                    .expect("typed result");
            assert!(
                result
                    .keys
                    .iter()
                    .any(|value| value == "GITHUB_ACCESS_TOKEN")
            );
            assert_eq!(result.token.as_deref(), Some("gho_test_123"));
        }
        other => panic!("unexpected tool response content: {other:?}"),
    }
}

#[tokio::test]
async fn actors_send_message_enqueues_actor_to_actor_mail() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );

    let source_actor_id = uri!("borg", "actor", "source-send");
    let source_session_id = uri!("borg", "session", "source-send");
    let source_user_id = uri!("borg", "user", "source-send");
    let target_actor_id = uri!("borg", "actor", "target-send");
    let target_session_id = uri!("borg", "session", "target-send");

    db.upsert_actor(&source_actor_id, "source-send", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&source_actor_id, "gpt-4o-mini")
        .await
        .unwrap();
    db.upsert_actor(&target_actor_id, "target-send", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&target_actor_id, "gpt-4o-mini")
        .await
        .unwrap();

    let toolchain = runtime
        .build_toolchain(&source_user_id, &source_session_id, &source_actor_id)
        .await
        .unwrap();

    let response = toolchain
        .run(ToolRequest {
            tool_call_id: "tool-call-send-1".to_string(),
            tool_name: "Actors-sendMessage".to_string(),
            arguments: json!({
                "target_actor_id": target_actor_id,
                "target_session_id": target_session_id,
                "text": "ping"
            })
            .into(),
        })
        .await
        .unwrap();

    let result = match response.content {
        ToolResultData::Text(text) => {
            serde_json::from_str::<ActorsSendMessageResult>(&text).expect("valid send response")
        }
        other => panic!("unexpected tool response content: {other:?}"),
    };
    assert_eq!(result.status, "delivered");
    assert_eq!(result.actor_message_id, result.submission_id);

    let row = sqlx::query!(
        r#"
        SELECT
            sender_id as "sender_id: String",
            receiver_id as "receiver_id!: String",
            session_id as "session_id: String",
            reply_to_message_id as "reply_to_message_id: String",
            status as "status!: String"
        FROM messages
        WHERE message_id = ?1
        LIMIT 1
        "#,
        result.actor_message_id,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();

    assert_eq!(row.sender_id.as_deref(), Some(source_actor_id.as_str()));
    assert_eq!(row.receiver_id, target_actor_id.to_string());
    assert_eq!(row.session_id.as_deref(), Some(target_session_id.as_str()));
    assert!(row.reply_to_message_id.is_none());
    assert_eq!(row.status, "QUEUED");
}

#[tokio::test]
async fn actors_receive_claims_and_acks_correlated_reply() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );

    let source_actor_id = uri!("borg", "actor", "source-receive");
    let source_session_id = uri!("borg", "session", "source-receive");
    let source_user_id = uri!("borg", "user", "source-receive");
    let target_actor_id = uri!("borg", "actor", "target-receive");
    let target_session_id = uri!("borg", "session", "target-receive");

    db.upsert_actor(&source_actor_id, "source-receive", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&source_actor_id, "gpt-4o-mini")
        .await
        .unwrap();
    db.upsert_actor(&target_actor_id, "target-receive", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&target_actor_id, "gpt-4o-mini")
        .await
        .unwrap();

    let toolchain = runtime
        .build_toolchain(&source_user_id, &source_session_id, &source_actor_id)
        .await
        .unwrap();

    let send_response = toolchain
        .run(ToolRequest {
            tool_call_id: "tool-call-send-2".to_string(),
            tool_name: "Actors-sendMessage".to_string(),
            arguments: json!({
                "target_actor_id": target_actor_id,
                "target_session_id": target_session_id,
                "text": "ping"
            })
            .into(),
        })
        .await
        .unwrap();

    let send_result = match send_response.content {
        ToolResultData::Text(text) => {
            serde_json::from_str::<ActorsSendMessageResult>(&text).expect("valid send response")
        }
        other => panic!("unexpected tool response content: {other:?}"),
    };
    let expected_submission =
        Uri::parse(send_result.submission_id.as_str()).expect("valid submission id");

    let reply_envelope = ActorMailboxEnvelope {
        actor_id: source_actor_id.to_string(),
        user_id: source_user_id.to_string(),
        session_id: source_session_id.to_string(),
        port_context: crate::PortContext::Unknown,
        input: crate::mailbox_envelope::ActorMailboxInput::Chat {
            text: "pong".to_string(),
        },
    };
    let reply_payload = serde_json::to_value(reply_envelope).unwrap();
    let reply_message_id = db
        .enqueue_actor_message_from_sender(
            Some(&target_actor_id),
            &source_actor_id,
            Some(&source_session_id),
            &reply_payload,
            None,
            Some(&expected_submission),
        )
        .await
        .unwrap();

    let receive_response = toolchain
        .run(ToolRequest {
            tool_call_id: "tool-call-receive-1".to_string(),
            tool_name: "Actors-receive".to_string(),
            arguments: json!({
                "expected_submission_id": send_result.submission_id,
                "timeout_ms": 2000
            })
            .into(),
        })
        .await
        .unwrap();

    let receive_result = match receive_response.content {
        ToolResultData::Text(text) => {
            serde_json::from_str::<ActorsReceiveResult>(&text).expect("valid receive response")
        }
        other => panic!("unexpected tool response content: {other:?}"),
    };

    assert_eq!(receive_result.status, "completed");
    assert_eq!(
        receive_result.actor_message_id,
        reply_message_id.to_string()
    );
    assert_eq!(receive_result.submission_id, reply_message_id.to_string());
    assert_eq!(
        receive_result.in_reply_to_submission_id.as_deref(),
        Some(expected_submission.as_str())
    );
    assert_eq!(
        receive_result.source_actor_id.as_deref(),
        Some(target_actor_id.as_str())
    );
    assert_eq!(receive_result.text, "pong");

    let reply_message_id_raw = reply_message_id.to_string();
    let row = sqlx::query!(
        r#"
        SELECT status as "status!: String"
        FROM messages
        WHERE message_id = ?1
        LIMIT 1
        "#,
        reply_message_id_raw,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.status, "ACKED");
}

#[tokio::test]
async fn borg_supervisor_creation_and_lifecycle() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);

    supervisor.start().await.unwrap();
    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_actor_rejects_cross_session_delivery() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "multi-session");
    db.upsert_actor(&actor_id, "multi-session", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();
    let user_id = uri!("borg", "user", "tester");
    let pctx = crate::PortContext::Unknown;
    let session_a = uri!("borg", "session", "a");
    let session_b = uri!("borg", "session", "b");

    let out_a = supervisor
        .call(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id: user_id.clone(),
            session_id: session_a.clone(),
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx.clone(),
        })
        .await
        .unwrap();

    let out_b = supervisor
        .call(crate::BorgMessage {
            actor_id,
            user_id,
            session_id: session_b.clone(),
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx,
        })
        .await;

    assert_eq!(out_a.session_id, session_a);
    assert!(out_b.is_err());
    assert!(
        out_b
            .expect_err("cross-session delivery should be rejected")
            .to_string()
            .contains("is bound to session")
    );

    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_persists_call_and_cast_to_actor_mailbox() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "mailbox-persist");
    db.upsert_actor(&actor_id, "mailbox-persist", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();
    let user_id = uri!("borg", "user", "tester");
    let pctx = crate::PortContext::Unknown;
    let session_id = uri!("borg", "session", "persist");

    supervisor
        .cast(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id: user_id.clone(),
            session_id: session_id.clone(),
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx.clone(),
        })
        .await
        .unwrap();

    supervisor
        .call(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id,
            session_id,
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx,
        })
        .await
        .unwrap();

    let actor_id_raw = actor_id.to_string();
    let total_count = sqlx::query!(
        r#"SELECT COUNT(*) as "n!: i64"
        FROM messages
        WHERE receiver_id = ?1
        "#,
        actor_id_raw,
    )
    .fetch_one(db.pool())
    .await
    .unwrap()
    .n;
    assert!(total_count >= 2);

    let mut acked = 0_i64;
    for _ in 0..20 {
        let actor_id_raw = actor_id.to_string();
        acked = sqlx::query!(
            r#"SELECT COUNT(*) as "n!: i64"
            FROM messages
            WHERE receiver_id = ?1 AND status = 'ACKED'
            "#,
            actor_id_raw,
        )
        .fetch_one(db.pool())
        .await
        .unwrap()
        .n;
        if acked >= 2 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert!(acked >= 2);

    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_missing_actor_spec_keeps_message_queued() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "missing-spec");
    let result = supervisor
        .cast(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id: uri!("borg", "user", "tester"),
            session_id: uri!("borg", "session", "missing-spec"),
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: crate::PortContext::Unknown,
        })
        .await;
    assert!(result.is_err());

    let actor_id_raw = actor_id.to_string();
    let row = sqlx::query!(
        r#"SELECT status as "status!: String"
        FROM messages
        WHERE receiver_id = ?1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        actor_id_raw,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let status = row.status;
    assert_eq!(status, "QUEUED");

    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_allows_model_set_before_actor_model_exists() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "model-init");
    db.upsert_actor(&actor_id, "model-init", "prompt", "RUNNING")
        .await
        .unwrap();

    let output = supervisor
        .call(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id: uri!("borg", "user", "tester"),
            session_id: uri!("borg", "session", "model-init"),
            input: crate::BorgInput::Command(crate::BorgCommand::ModelSet {
                model: "gpt-4o-mini".to_string(),
            }),
            port_context: crate::PortContext::Unknown,
        })
        .await
        .unwrap();

    let reply = output.reply.unwrap_or_default();
    assert!(reply.contains("Updated model to gpt-4o-mini"));

    let actor = db
        .get_actor(&actor_id)
        .await
        .unwrap()
        .expect("actor must exist");
    assert_eq!(actor.model.as_deref(), Some("gpt-4o-mini"));

    supervisor.shutdown().await;
}

#[tokio::test]
async fn audio_turn_rejects_without_transcription_provider_and_persists_no_user_audio_message() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let files = BorgFs::local(db.clone(), temp_files_root());
    let session_id = uri!("borg", "session", "audio-reject");
    let file = files
        .put_bytes(
            FileKind::Audio,
            b"fake-audio-bytes",
            PutFileMetadata {
                session_id: session_id.clone(),
            },
        )
        .await
        .unwrap();

    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        files,
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "audio-reject");
    db.upsert_actor(&actor_id, "audio-reject", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();

    let result = supervisor
        .call(crate::BorgMessage {
            actor_id,
            user_id: uri!("borg", "user", "tester"),
            session_id: session_id.clone(),
            input: crate::BorgInput::Audio {
                file_id: file.file_id,
                mime_type: Some("audio/wav".to_string()),
                duration_ms: Some(2500),
                language_hint: Some("en".to_string()),
            },
            port_context: crate::PortContext::Unknown,
        })
        .await;
    assert!(result.is_err());

    let messages = db.list_session_messages(&session_id, 0, 50).await.unwrap();
    assert!(messages.is_empty());

    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_replays_queued_after_actor_spec_is_created() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);
    supervisor.start().await.unwrap();

    let actor_id = uri!("devmode", "actor", "late-created");
    let user_id = uri!("borg", "user", "tester");
    let session_id = uri!("borg", "session", "late-created");
    let pctx = crate::PortContext::Unknown;

    let queued_err = supervisor
        .cast(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id: user_id.clone(),
            session_id: session_id.clone(),
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx.clone(),
        })
        .await
        .expect_err("cast should fail before actor spec exists");
    assert!(queued_err.to_string().contains("actor spec not found"));

    db.upsert_actor(&actor_id, "late-created", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();

    supervisor
        .cast(crate::BorgMessage {
            actor_id: actor_id.clone(),
            user_id,
            session_id,
            input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
            port_context: pctx,
        })
        .await
        .unwrap();

    let mut queued = 2_i64;
    for _ in 0..50 {
        let actor_id_raw = actor_id.to_string();
        queued = sqlx::query!(
            r#"SELECT COUNT(*) as "n!: i64"
            FROM messages
            WHERE receiver_id = ?1 AND status = 'QUEUED'
            "#,
            actor_id_raw,
        )
        .fetch_one(db.pool())
        .await
        .unwrap()
        .n;
        if queued == 0 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(queued, 0);

    supervisor.shutdown().await;
}

#[tokio::test]
async fn borg_supervisor_start_fails_stale_in_progress_mailbox_rows() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);

    let actor_id = uri!("devmode", "actor", "stale-fail");
    db.upsert_actor(&actor_id, "stale-fail", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();

    let msg_id = db
        .enqueue_actor_message(&actor_id, None, &json!({"x":1}), None, None)
        .await
        .unwrap();
    let _claimed = db
        .claim_next_actor_message(&actor_id)
        .await
        .unwrap()
        .expect("claimed");

    let msg_id_raw = msg_id.to_string();
    sqlx::query!(
        "UPDATE messages SET started_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 hour') WHERE message_id = ?1",
        msg_id_raw,
    )
    .execute(db.pool())
    .await
    .unwrap();

    supervisor.start().await.unwrap();

    let msg_id_raw = msg_id.to_string();
    let row = sqlx::query!(
        r#"SELECT status as "status!: String"
        FROM messages
        WHERE message_id = ?1
        LIMIT 1
        "#,
        msg_id_raw,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let status = row.status;
    assert_eq!(status, "FAILED");
}

#[tokio::test]
async fn borg_supervisor_start_replays_queued_mailbox_rows() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let runtime = crate::BorgRuntime::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        BorgFs::local(db.clone(), temp_files_root()),
    );
    let runtime = std::sync::Arc::new(runtime);
    let supervisor = BorgSupervisor::new(runtime);

    let actor_id = uri!("devmode", "actor", "replay-queued");
    db.upsert_actor(&actor_id, "replay-queued", "prompt", "RUNNING")
        .await
        .unwrap();
    db.set_actor_model(&actor_id, "gpt-4o-mini").await.unwrap();
    let msg = crate::BorgMessage {
        actor_id: actor_id.clone(),
        user_id: uri!("borg", "user", "tester"),
        session_id: uri!("borg", "session", "replay"),
        input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
        port_context: crate::PortContext::Unknown,
    };
    let payload = serde_json::to_value(ActorMailboxEnvelope::from_borg_message(&msg)).unwrap();
    let msg_id = db
        .enqueue_actor_message(&actor_id, Some(&msg.session_id), &payload, None, None)
        .await
        .unwrap();

    supervisor.start().await.unwrap();

    let mut status = String::new();
    for _ in 0..25 {
        let msg_id_raw = msg_id.to_string();
        status = sqlx::query!(
            r#"SELECT status as "status!: String"
            FROM messages
            WHERE message_id = ?1
            LIMIT 1
            "#,
            msg_id_raw,
        )
        .fetch_one(db.pool())
        .await
        .unwrap()
        .status;
        if status == "ACKED" {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(status, "ACKED");

    supervisor.shutdown().await;
}
