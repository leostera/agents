use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

use borg_core::{Event, TaskKind, TaskStatus, Uri, uri};
use borg_db::{BorgDb, NewTask};
use borg_rt::CodeModeRuntime;
use serde_json::json;
use tracing_subscriber::EnvFilter;

use crate::{BorgExecutor, UserMessage};

const OPENAI_PROVIDER: &str = "openai";

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("borg-exec-test-{}.db", uuid::Uuid::now_v7()))
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
    BorgExecutor::new(db, CodeModeRuntime::default(), Uri::parse(worker).unwrap())
        .with_openai_base_url(Some("http://127.0.0.1:1".to_string()))
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
