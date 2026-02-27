use std::path::PathBuf;
use std::sync::Once;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_agent::{Agent, AgentTools, Message, Session, SessionResult, ToolResultData};
use borg_core::{Event, TaskKind, TaskStatus, Uri, uri};
use borg_db::{BorgDb, NewTask};
use borg_llm::{
    LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason,
    TranscriptionRequest,
};
use borg_ltm::MemoryStore;
use borg_rt::{CodeModeRuntime, default_tool_specs};
use serde_json::json;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::tool_runner::build_exec_toolchain;
use crate::{BorgExecutor, UserMessage};

const OPENAI_PROVIDER: &str = "openai";
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

async fn failing_openai_exec(db: BorgDb, worker: &str) -> BorgExecutor {
    db.upsert_provider_api_key(OPENAI_PROVIDER, "test-key")
        .await
        .unwrap();
    let memory = open_test_memory().await;
    BorgExecutor::new(
        db,
        memory,
        CodeModeRuntime::default(),
        Uri::parse(worker).unwrap(),
    )
    .with_openai_base_url(Some("http://127.0.0.1:1".to_string()))
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
    BorgExecutor::new(
        db,
        memory,
        CodeModeRuntime::default(),
        Uri::parse(worker).unwrap(),
    )
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

async fn wait_for_task_status(
    db: &BorgDb,
    task_id: &Uri,
    status: TaskStatus,
    timeout: Duration,
) -> bool {
    let start = std::time::Instant::now();
    loop {
        let task = db.get_task(task_id).await.unwrap();
        if let Some(task) = task {
            if task.status == status {
                return true;
            }
        }
        if start.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn enqueue_user_message_persists_task_and_session() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "borg:worker:test").await;

    let msg = UserMessage {
        user_key: Uri::parse("borg:user:u1").unwrap(),
        text: "hello".to_string(),
        session_id: None,
        agent_id: None,
        metadata: json!({}),
    };
    let (task_id, session_id) = exec.enqueue_user_message(msg, None).await.unwrap();

    let task = db.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.kind, TaskKind::UserMessage);
    assert!(session_id.as_str().starts_with("borg:session:"));
}

#[tokio::test]
async fn run_processes_enqueued_task_and_marks_failed_when_provider_unreachable() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "borg:worker:no-provider").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = UserMessage {
        user_key: Uri::parse("borg:user:u2").unwrap(),
        text: "hello from test".to_string(),
        session_id: None,
        agent_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type.as_str() == "borg:task:created")
    );
    assert!(
        events
            .iter()
            .any(|e| e.event_type.as_str() == "borg:task:claimed")
    );
    assert!(
        events
            .iter()
            .any(|e| e.event_type.as_str() == "borg:task:failed")
    );

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn session_resumes_same_agent_id_when_message_omits_agent() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "borg:worker:session").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let session_id = uri!("borg", "session");
    let custom_agent = uri!("borg", "agent", "support");
    db.upsert_agent_spec(&custom_agent, "gpt-4o-mini", "You are support.", &json!([]))
        .await
        .unwrap();

    let first = UserMessage {
        user_key: uri!("borg", "user", "u4"),
        text: "first".to_string(),
        session_id: Some(session_id.clone()),
        agent_id: Some(custom_agent.clone()),
        metadata: json!({}),
    };
    let second = UserMessage {
        user_key: uri!("borg", "user", "u4"),
        text: "second".to_string(),
        session_id: Some(session_id.clone()),
        agent_id: None,
        metadata: json!({}),
    };

    let (task_a, _) = exec
        .enqueue_user_message(first, Some(session_id.clone()))
        .await
        .unwrap();
    let (task_b, _) = exec
        .enqueue_user_message(second, Some(session_id.clone()))
        .await
        .unwrap();

    let done_a =
        wait_for_task_status(&db, &task_a, TaskStatus::Failed, Duration::from_secs(5)).await;
    let done_b =
        wait_for_task_status(&db, &task_b, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done_a);
    assert!(done_b);

    let messages = db.list_session_messages(&session_id, 0, 64).await.unwrap();
    let started_agent_ids: Vec<String> = messages
        .into_iter()
        .filter_map(|value| serde_json::from_value::<borg_agent::Message>(value).ok())
        .filter_map(|message| match message {
            borg_agent::Message::SessionEvent {
                payload: borg_agent::SessionEventPayload::Started { agent_id },
                ..
            } => Some(agent_id.to_string()),
            _ => None,
        })
        .collect();
    assert!(started_agent_ids.contains(&custom_agent.to_string()));

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn context_built_event_contains_system_prompt_and_tool_schema() {
    init_test_tracing();
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "borg:worker:context").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = UserMessage {
        user_key: uri!("borg", "user", "u5"),
        text: "context please".to_string(),
        session_id: None,
        agent_id: None,
        metadata: json!({}),
    };

    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();
    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    let context_event = events
        .iter()
        .filter_map(|event| serde_json::from_value::<Event>(event.payload.clone()).ok())
        .find_map(|event| match event {
            Event::ContextBuilt { context, .. } => Some(context),
            _ => None,
        })
        .unwrap();

    assert!(
        context_event
            .messages
            .iter()
            .any(|message| message.to_string().contains("system"))
    );
    assert!(!context_event.tools.is_empty());

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn task_event_order_is_deterministic_for_failed_run() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "borg:worker:ordering").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = UserMessage {
        user_key: uri!("borg", "user", "u7"),
        text: "ordering".to_string(),
        session_id: None,
        agent_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    let names: Vec<String> = events
        .into_iter()
        .map(|event| event.event_type.to_string())
        .collect();

    let idx_created = names
        .iter()
        .position(|name| name == "borg:task:created")
        .unwrap();
    let idx_claimed = names
        .iter()
        .position(|name| name == "borg:task:claimed")
        .unwrap();
    let idx_context = names
        .iter()
        .position(|name| name == "borg:session:context_built")
        .unwrap();
    let idx_llm_req = names
        .iter()
        .position(|name| name == "borg:llm:request_sent")
        .unwrap();
    let idx_failed = names
        .iter()
        .position(|name| name == "borg:task:failed")
        .unwrap();

    assert!(idx_created < idx_claimed);
    assert!(idx_claimed < idx_context);
    assert!(idx_context < idx_llm_req);
    assert!(idx_llm_req < idx_failed);

    handle.abort();
    let _ = handle.await;
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
            .contains("OpenAI provider key is required for transcription"),
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
        uri!("borg", "worker", "set-model-existing"),
    );
    let default_agent_id = uri!("borg", "agent", "default");
    let existing_tools = json!([{
        "name": "tool-a",
        "description": "demo",
        "parameters": { "type": "object" }
    }]);
    db.upsert_agent_spec(
        &default_agent_id,
        "gpt-4o-mini",
        "Keep this system prompt.",
        &existing_tools,
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
    assert_eq!(updated.tools, existing_tools);
}

#[tokio::test]
async fn set_model_for_session_creates_default_agent_spec_when_missing() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db.clone(),
        memory,
        CodeModeRuntime::default(),
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
    assert!(
        created
            .tools
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
    );
}

#[tokio::test]
async fn set_model_for_session_rejects_empty_model() {
    let db = open_test_db().await;
    let memory = open_test_memory().await;
    let exec = BorgExecutor::new(
        db,
        memory,
        CodeModeRuntime::default(),
        uri!("borg", "worker", "set-model-invalid"),
    );

    let session_id = uri!("borg", "session", "model-update-invalid");
    let err = exec
        .set_model_for_session(&session_id, "   ")
        .await
        .expect_err("empty model must fail");
    assert!(err.to_string().contains("model must not be empty"));
}

#[tokio::test]
async fn recover_running_task_is_requeued_and_processed() {
    let db = open_test_db().await;
    let payload = json!(UserMessage {
        user_key: uri!("borg", "user", "u3"),
        text: "recover me".to_string(),
        session_id: Some(uri!("borg", "session")),
        agent_id: None,
        metadata: json!({}),
    });

    let task_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::UserMessage,
            payload,
            parent_task_id: None,
        })
        .await
        .unwrap();

    let worker = uri!("borg", "worker", "old-worker");
    let claimed = db.claim_next_runnable_task(&worker).await.unwrap().unwrap();
    assert_eq!(claimed.task_id, task_id);

    let exec = failing_openai_exec(db.clone(), "borg:worker:recovery").await;
    let handle = tokio::spawn(async move { exec.run().await.unwrap() });

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    handle.abort();
    let _ = handle.await;
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
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(req.clone());
        self.responses_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow!("scripted provider exhausted"))?
            .map_err(|err| anyhow!(err))
    }

    async fn transcribe(&self, _req: &TranscriptionRequest) -> Result<String> {
        Err(anyhow!("transcribe not supported in scripted provider"))
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
            "searchApis",
            json!({ "query": "ls fetch" }),
        )),
        Ok(assistant_tool_call(
            "call_exec_1",
            "executeCode",
            json!({
                "hint": "Inspecting working directory entries",
                "code": "async () => { const listing = Borg.OS.ls('.'); return { has_entries: listing.entries.length > 0, first_entry: listing.entries[0] ?? null }; }"
            }),
        )),
        Ok(assistant_text(
            "Completed runtime plan. BORG_EXEC_TOOLCHAIN_RT_OK",
        )),
    ]);

    let toolchain =
        build_exec_toolchain(CodeModeRuntime::default(), open_test_memory().await).unwrap();
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
    assert!(
        messages.iter().any(
            |message| matches!(message, Message::ToolCall { name, .. } if name == "searchApis")
        )
    );
    assert!(
        messages.iter().any(
            |message| matches!(message, Message::ToolCall { name, .. } if name == "executeCode")
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
            ProviderMessage::ToolResult { name, .. } if name == "searchApis"
        )
    }));
    assert!(requests[2].messages.iter().any(|message| {
        matches!(
            message,
            ProviderMessage::ToolResult { name, .. } if name == "executeCode"
        )
    }));
}

#[tokio::test]
async fn e2e_agent_toolchain_runtime_invalid_execute_returns_tool_error_and_recovers() {
    init_test_tracing();
    let db = open_test_db().await;
    let agent = Agent::new(uri!("borg", "agent", "exec-e2e-invalid"))
        .with_system_prompt("Call executeCode and then summarize the outcome.")
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
            "executeCode",
            json!({
                "hint": "Running invalid execute payload for error handling test",
                "code": "Borg.OS.ls('.')"
            }),
        )),
        Ok(assistant_text("Saw tool failure and handled it.")),
    ]);

    let toolchain =
        build_exec_toolchain(CodeModeRuntime::default(), open_test_memory().await).unwrap();
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
