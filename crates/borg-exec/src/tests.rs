use std::path::PathBuf;
use std::sync::Once;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use borg_agent::{Agent, AgentTools, Message, Session, SessionResult, ToolResultData};
use borg_codemode::{CodeModeContext, CodeModeRuntime, default_tool_specs};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    TranscriptionRequest,
};
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use serde_json::json;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::BorgExecutor;
use crate::tool_runner::build_exec_toolchain_with_context;

const OPENROUTER_PROVIDER: &str = "openrouter";
const RUNTIME_SETTINGS_PORT: &str = "runtime";
const RUNTIME_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("borg-exec-test-{}.db", uuid::Uuid::now_v7()))
}

fn temp_memory_paths() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("borg-exec-ltm-{}", uuid::Uuid::now_v7()));
    let search = std::env::temp_dir().join(format!("borg-exec-search-{}", uuid::Uuid::now_v7()));
    (root, search)
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

async fn openrouter_exec_without_openai_transcription_fallback(
    db: BorgDb,
    worker: &str,
) -> BorgExecutor {
    db.upsert_provider_api_key(OPENROUTER_PROVIDER, "test-openrouter-key")
        .await
        .unwrap();
    db.upsert_port_setting(
        RUNTIME_SETTINGS_PORT,
        RUNTIME_PREFERRED_PROVIDER_KEY,
        OPENROUTER_PROVIDER,
    )
    .await
    .unwrap();
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db,
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        Uri::parse(worker).unwrap(),
    );
    {
        let provider_settings = exec.provider_settings_handle();
        let mut settings = provider_settings.write().await;
        settings.openrouter_api_key = Some("test-openrouter-key".to_string());
        settings.preferred_provider = Some(OPENROUTER_PROVIDER.to_string());
    }
    exec
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

#[tokio::test]
async fn openrouter_transcription_requires_openai_fallback_key() {
    let db = open_test_db().await;
    let exec =
        openrouter_exec_without_openai_transcription_fallback(db, "borg:worker:openrouter").await;

    let err = exec
        .transcribe_audio(vec![0x01, 0x02], "audio/ogg")
        .await
        .expect_err("transcription should fail without openai fallback");
    assert!(
        err.to_string()
            .contains("OpenAI provider key is required for transcription")
            || err
                .to_string()
                .contains("audio transcription model is required"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn set_model_for_session_updates_existing_agent_spec_and_preserves_fields() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        uri!("borg", "worker", "set-model-existing"),
    );
    let default_agent_id = uri!("borg", "agent", "default");
    db.upsert_agent_spec(
        &default_agent_id,
        "Default Agent",
        Some("openrouter"),
        "gpt-4o-mini",
        "Keep this system prompt.",
    )
    .await
    .unwrap();

    let session_id = uri!("borg", "session", "model-update-existing");
    let (agent_id, model) = exec
        .set_model_for_session(&session_id, "openai/gpt-4.1-mini")
        .await
        .unwrap();

    assert_eq!(agent_id, default_agent_id);
    assert_eq!(model, "openai/gpt-4.1-mini");

    let updated = db.get_agent_spec(&default_agent_id).await.unwrap().unwrap();
    assert_eq!(updated.model, "openai/gpt-4.1-mini");
    assert_eq!(updated.system_prompt, "Keep this system prompt.");
    assert_eq!(updated.default_provider_id.as_deref(), Some("openrouter"));
}

#[tokio::test]
async fn set_model_for_session_creates_default_agent_spec_when_missing() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        uri!("borg", "worker", "set-model-new"),
    );

    let session_id = uri!("borg", "session", "model-update-new");
    let (agent_id, model) = exec
        .set_model_for_session(&session_id, "openai/gpt-4.1-nano")
        .await
        .unwrap();

    assert_eq!(agent_id, uri!("borg", "agent", "default"));
    assert_eq!(model, "openai/gpt-4.1-nano");

    let created = db.get_agent_spec(&agent_id).await.unwrap().unwrap();
    assert_eq!(created.model, "openai/gpt-4.1-nano");
    assert!(!created.system_prompt.is_empty());
    assert_eq!(created.default_provider_id, None);
}

#[tokio::test]
async fn set_model_for_session_rejects_empty_model() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db,
        memory,
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        uri!("borg", "worker", "set-model-invalid"),
    );

    let session_id = uri!("borg", "session", "model-update-invalid");
    let err = exec
        .set_model_for_session(&session_id, "   ")
        .await
        .expect_err("empty model must fail");
    assert!(err.to_string().contains("model must not be empty"));
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

#[tokio::test]
async fn e2e_agent_toolchain_runtime_search_then_execute_then_reply() {
    init_test_tracing();
    let db = open_test_db().await;
    let agent = Agent::new(uri!("borg", "agent", "exec-e2e"))
        .with_system_prompt("Use tools when needed and provide a final concise answer.")
        .with_tools(default_tool_specs());
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

    let toolchain = build_exec_toolchain_with_context(
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        CodeModeContext::default(),
        open_test_memory().await,
        open_test_db().await,
    )
    .unwrap();
    let tools = AgentTools {
        tool_runner: &toolchain,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
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
        .with_tools(default_tool_specs());
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

    let toolchain = build_exec_toolchain_with_context(
        CodeModeRuntime::default(),
        ShellModeRuntime::new(),
        CodeModeContext::default(),
        open_test_memory().await,
        open_test_db().await,
    )
    .unwrap();
    let tools = AgentTools {
        tool_runner: &toolchain,
    };

    let result = agent.run(&mut session, &provider, &tools).await;
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
