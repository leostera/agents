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
use borg_core::uri;
use borg_db::BorgDb;
use borg_fs::{BorgFs, FileKind, PutFileMetadata};
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    TranscriptionRequest,
};
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use serde_json::json;
use sqlx::Row;
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
    args: serde_json::Value,
) -> LlmAssistantMessage {
    LlmAssistantMessage {
        content: vec![ProviderBlock::ToolCall {
            id: tool_call_id.to_string(),
            name: name.to_string(),
            arguments_json: args,
        }],
        stop_reason: StopReason::ToolCall,
        error_message: None,
        usage_tokens: None,
    }
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
    let manager = SessionManager::new(db.clone(), "gpt-4o-mini".to_string());
    let agent_id = uri!("borg", "agent", "refresh-test");

    db.upsert_agent_spec(&agent_id, "refresh-test", None, "gpt-4o-mini", "prompt-v1")
        .await
        .unwrap();

    let first = manager
        .resolve_agent_for_turn(&agent_id, None)
        .await
        .unwrap();
    assert_eq!(first.system_prompt, "prompt-v1");
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
    db.upsert_agent_spec(&agent_id, "refresh-test", None, "gpt-4o-mini", "prompt-v2")
        .await
        .unwrap();

    let second = manager
        .resolve_agent_for_turn(&agent_id, None)
        .await
        .unwrap();
    assert_eq!(second.system_prompt, "prompt-v2");
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
            json!({ "query": "ls fetch" }),
        )),
        Ok(assistant_tool_call(
            "call_exec_1",
            "CodeMode-executeCode",
            json!({
                "hint": "Inspecting working directory entries",
                "code": "async () => { const listing = Borg.OS.ls('.'); return { has_entries: listing.entries.length > 0, first_entry: listing.entries[0] ?? null }; }"
            }),
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
            } if result.get("has_entries").is_some()
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
            }),
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
            }),
        })
        .await
        .unwrap();

    match response.content {
        ToolResultData::Execution { result, .. } => {
            let keys = result
                .get("keys")
                .and_then(serde_json::Value::as_array)
                .expect("keys array");
            assert!(
                keys.iter()
                    .any(|value| value.as_str() == Some("GITHUB_ACCESS_TOKEN"))
            );
            assert_eq!(
                result.get("token").and_then(serde_json::Value::as_str),
                Some("gho_test_123")
            );
        }
        other => panic!("unexpected tool response content: {other:?}"),
    }
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
async fn borg_supervisor_actor_can_serve_multiple_sessions() {
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
    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(
        &actor_id,
        "multi-session",
        "prompt",
        &behavior_id,
        "RUNNING",
    )
    .await
    .unwrap();
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
        .await
        .unwrap();

    assert_eq!(out_a.session_id, session_a);
    assert_eq!(out_b.session_id, session_b);

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
    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(
        &actor_id,
        "mailbox-persist",
        "prompt",
        &behavior_id,
        "RUNNING",
    )
    .await
    .unwrap();
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

    let cast_count = sqlx::query(
        "SELECT COUNT(*) as n FROM actor_mailbox WHERE actor_id = ?1 AND kind = 'CAST'",
    )
    .bind(actor_id.to_string())
    .fetch_one(db.pool())
    .await
    .unwrap()
    .try_get::<i64, _>("n")
    .unwrap();
    let call_count = sqlx::query(
        "SELECT COUNT(*) as n FROM actor_mailbox WHERE actor_id = ?1 AND kind = 'CALL'",
    )
    .bind(actor_id.to_string())
    .fetch_one(db.pool())
    .await
    .unwrap()
    .try_get::<i64, _>("n")
    .unwrap();

    assert!(cast_count >= 1);
    assert!(call_count >= 1);

    let mut acked = 0_i64;
    for _ in 0..20 {
        acked = sqlx::query(
            "SELECT COUNT(*) as n FROM actor_mailbox WHERE actor_id = ?1 AND status = 'ACKED'",
        )
        .bind(actor_id.to_string())
        .fetch_one(db.pool())
        .await
        .unwrap()
        .try_get::<i64, _>("n")
        .unwrap();
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

    let status: String = sqlx::query(
        "SELECT status FROM actor_mailbox WHERE actor_id = ?1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(actor_id.to_string())
    .fetch_one(db.pool())
    .await
    .unwrap()
    .try_get("status")
    .unwrap();
    assert_eq!(status, "QUEUED");

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
    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(&actor_id, "audio-reject", "prompt", &behavior_id, "RUNNING")
        .await
        .unwrap();

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

    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(&actor_id, "late-created", "prompt", &behavior_id, "RUNNING")
        .await
        .unwrap();

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
        queued = sqlx::query(
            "SELECT COUNT(*) as n FROM actor_mailbox WHERE actor_id = ?1 AND status = 'QUEUED'",
        )
        .bind(actor_id.to_string())
        .fetch_one(db.pool())
        .await
        .unwrap()
        .try_get::<i64, _>("n")
        .unwrap();
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
    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(&actor_id, "stale-fail", "prompt", &behavior_id, "RUNNING")
        .await
        .unwrap();

    let msg_id = db
        .enqueue_actor_message(&actor_id, "CAST", None, &json!({"x":1}), None, None)
        .await
        .unwrap();
    let _claimed = db
        .claim_next_actor_message(&actor_id)
        .await
        .unwrap()
        .expect("claimed");

    sqlx::query(
        "UPDATE actor_mailbox SET started_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 hour') WHERE actor_message_id = ?1",
    )
        .bind(msg_id.to_string())
        .execute(db.pool())
        .await
        .unwrap();

    supervisor.start().await.unwrap();

    let status: String =
        sqlx::query("SELECT status FROM actor_mailbox WHERE actor_message_id = ?1 LIMIT 1")
            .bind(msg_id.to_string())
            .fetch_one(db.pool())
            .await
            .unwrap()
            .try_get("status")
            .unwrap();
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
    let behavior_id = uri!("borg", "behavior", "default");
    db.upsert_actor(
        &actor_id,
        "replay-queued",
        "prompt",
        &behavior_id,
        "RUNNING",
    )
    .await
    .unwrap();
    let msg = crate::BorgMessage {
        actor_id: actor_id.clone(),
        user_id: uri!("borg", "user", "tester"),
        session_id: uri!("borg", "session", "replay"),
        input: crate::BorgInput::Command(crate::BorgCommand::ContextDump),
        port_context: crate::PortContext::Unknown,
    };
    let payload = serde_json::to_value(ActorMailboxEnvelope::from_borg_message(&msg)).unwrap();
    let msg_id = db
        .enqueue_actor_message(
            &actor_id,
            "CAST",
            Some(&msg.session_id),
            &payload,
            None,
            None,
        )
        .await
        .unwrap();

    supervisor.start().await.unwrap();

    let mut status = String::new();
    for _ in 0..25 {
        status =
            sqlx::query("SELECT status FROM actor_mailbox WHERE actor_message_id = ?1 LIMIT 1")
                .bind(msg_id.to_string())
                .fetch_one(db.pool())
                .await
                .unwrap()
                .try_get::<String, _>("status")
                .unwrap();
        if status == "ACKED" {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(status, "ACKED");

    supervisor.shutdown().await;
}
