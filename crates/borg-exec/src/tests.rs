use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

use borg_core::{TaskKind, TaskStatus};
use borg_db::{BorgDb, NewTask};
use borg_llm::testing::llm_container::LlmContainer;
use borg_rt::RuntimeEngine;
use serde_json::json;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::{BorgExecutor, InboxMessage};

const OPENAI_PROVIDER: &str = "openai";

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("borg-exec-test-{}.db", Uuid::now_v7()))
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
    BorgExecutor::new(db, RuntimeEngine::default(), worker.to_string())
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
    task_id: &str,
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
    let exec = failing_openai_exec(db.clone(), "worker-test").await;

    let msg = InboxMessage {
        user_key: "u1".to_string(),
        text: "hello".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let (task_id, session_id) = exec.enqueue_user_message(msg, None).await.unwrap();

    let task = db.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.kind, TaskKind::UserMessage);
    assert!(session_id.starts_with("borg:session:"));
}

#[tokio::test]
async fn run_processes_enqueued_task_and_marks_failed_when_provider_unreachable() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "worker-no-provider").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = InboxMessage {
        user_key: "u2".to_string(),
        text: "hello from test".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    assert!(events.iter().any(|e| e.event_type == "task_created"));
    assert!(events.iter().any(|e| e.event_type == "task_claimed"));
    assert!(events.iter().any(|e| e.event_type == "task_failed"));

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn run_recovers_running_tasks_on_startup_and_executes_them() {
    let db = open_test_db().await;

    let payload = json!(InboxMessage {
        user_key: "u3".to_string(),
        text: "recover me".to_string(),
        session_id: Some(format!("borg:session:{}", Uuid::now_v7())),
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

    let claimed = db
        .claim_next_runnable_task("old-worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.task_id, task_id);
    assert_eq!(claimed.status, TaskStatus::Running);

    db.upsert_provider_api_key(OPENAI_PROVIDER, "test-key")
        .await
        .unwrap();
    let exec = BorgExecutor::new(
        db.clone(),
        RuntimeEngine::default(),
        "recovery-worker".to_string(),
    )
    .with_openai_base_url(Some("http://127.0.0.1:1".to_string()));
    let handle = tokio::spawn(async move { exec.run().await.unwrap() });

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let task = db.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(task.claimed_by.unwrap(), "recovery-worker");
    assert!(task.last_error.is_some());

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn run_reuses_explicit_session_id_across_multiple_tasks() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "worker-session").await;

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let session_id = format!("borg:session:{}", Uuid::now_v7());
    let first = InboxMessage {
        user_key: "u4".to_string(),
        text: "first".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let second = InboxMessage {
        user_key: "u4".to_string(),
        text: "second".to_string(),
        session_id: None,
        metadata: json!({}),
    };

    let (task_a, returned_session_a) = exec
        .enqueue_user_message(first, Some(session_id.clone()))
        .await
        .unwrap();
    let (task_b, returned_session_b) = exec
        .enqueue_user_message(second, Some(session_id.clone()))
        .await
        .unwrap();

    assert_eq!(returned_session_a, session_id);
    assert_eq!(returned_session_b, session_id);

    let done_a =
        wait_for_task_status(&db, &task_a, TaskStatus::Failed, Duration::from_secs(5)).await;
    let done_b =
        wait_for_task_status(&db, &task_b, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done_a);
    assert!(done_b);

    let message_count = db.count_session_messages(&session_id).await.unwrap();
    assert!(message_count >= 2);

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn duplicate_queue_entries_only_one_claim_event_for_single_task() {
    let db = open_test_db().await;

    let task_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::UserMessage,
            payload: json!(InboxMessage {
                user_key: "u5".to_string(),
                text: "single-claim".to_string(),
                session_id: Some(format!("borg:session:{}", Uuid::now_v7())),
                metadata: json!({}),
            }),
            parent_task_id: None,
        })
        .await
        .unwrap();

    let exec = failing_openai_exec(db.clone(), "worker-a").await;
    exec.queue_task_id(task_id.clone()).await.unwrap();
    let handle = tokio::spawn(async move { exec.run().await.unwrap() });

    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    let claimed_events = events
        .iter()
        .filter(|e| e.event_type == "task_claimed")
        .count();
    assert_eq!(claimed_events, 1);

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn stale_task_ids_are_ignored_and_executor_continues() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "worker-stale").await;

    let stale_task = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"note":"stale"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    db.complete_task(&stale_task, json!({"message":"done"}))
        .await
        .unwrap();
    exec.queue_task_id(stale_task.clone()).await.unwrap();

    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = InboxMessage {
        user_key: "u6".to_string(),
        text: "after stale".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();
    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn recovery_requeues_only_running_not_terminal() {
    let db = open_test_db().await;

    let queued_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"k":"queued"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    let running_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"k":"running"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    let succeeded_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"k":"succeeded"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    let failed_id = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"k":"failed"}),
            parent_task_id: None,
        })
        .await
        .unwrap();

    let _ = db
        .claim_task_by_id("seed-worker", &running_id)
        .await
        .unwrap();
    db.complete_task(&succeeded_id, json!({"message":"ok"}))
        .await
        .unwrap();
    db.fail_task(&failed_id, "boom".to_string()).await.unwrap();

    let changed = db.requeue_running_tasks().await.unwrap();
    assert_eq!(changed, 1);

    let running_task = db.get_task(&running_id).await.unwrap().unwrap();
    let queued_task = db.get_task(&queued_id).await.unwrap().unwrap();
    let succeeded_task = db.get_task(&succeeded_id).await.unwrap().unwrap();
    let failed_task = db.get_task(&failed_id).await.unwrap().unwrap();
    assert_eq!(running_task.status, TaskStatus::Queued);
    assert_eq!(queued_task.status, TaskStatus::Queued);
    assert_eq!(succeeded_task.status, TaskStatus::Succeeded);
    assert_eq!(failed_task.status, TaskStatus::Failed);
}

#[tokio::test]
async fn dependency_unblocks_after_parent_succeeds() {
    let db = open_test_db().await;
    let parent = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"name":"parent"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    let child = db
        .enqueue_task(NewTask {
            kind: TaskKind::System,
            payload: json!({"name":"child"}),
            parent_task_id: None,
        })
        .await
        .unwrap();
    db.add_dependency(&child, &parent).await.unwrap();

    let exec = BorgExecutor::new(
        db.clone(),
        RuntimeEngine::default(),
        "worker-deps".to_string(),
    );
    let handle = tokio::spawn(async move { exec.run().await.unwrap() });

    let parent_done =
        wait_for_task_status(&db, &parent, TaskStatus::Succeeded, Duration::from_secs(5)).await;
    let child_done =
        wait_for_task_status(&db, &child, TaskStatus::Succeeded, Duration::from_secs(5)).await;
    assert!(parent_done);
    assert!(child_done);

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn failed_task_event_order_is_stable() {
    let db = open_test_db().await;
    let exec = failing_openai_exec(db.clone(), "worker-order").await;
    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = InboxMessage {
        user_key: "u7".to_string(),
        text: "event order".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();
    let done =
        wait_for_task_status(&db, &task_id, TaskStatus::Failed, Duration::from_secs(5)).await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    let names: Vec<String> = events.into_iter().map(|e| e.event_type).collect();
    assert!(names.len() >= 3);
    assert_eq!(names[0], "task_created");
    assert_eq!(names[1], "task_claimed");
    assert_eq!(names[names.len() - 1], "task_failed");

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn real_llm_path_marks_task_succeeded() {
    init_test_tracing();
    let llm = LlmContainer::start_ollama().await.unwrap();
    let db = open_test_db().await;
    db.upsert_provider_api_key(OPENAI_PROVIDER, &llm.api_key)
        .await
        .unwrap();

    let exec = BorgExecutor::new(
        db.clone(),
        RuntimeEngine::default(),
        "worker-real-llm".to_string(),
    )
    .with_openai_base_url(Some(llm.base_url.clone()))
    .with_agent_model(llm.model.clone());
    let runner = exec.clone();
    let handle = tokio::spawn(async move { runner.run().await.unwrap() });

    let msg = InboxMessage {
        user_key: "u8".to_string(),
        text: "Reply briefly with the token BORG_EXEC_LLM_OK".to_string(),
        session_id: None,
        metadata: json!({}),
    };
    let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();
    let done = wait_for_task_status(
        &db,
        &task_id,
        TaskStatus::Succeeded,
        Duration::from_secs(90),
    )
    .await;
    assert!(done);

    let events = db.get_task_events(&task_id).await.unwrap();
    assert!(events.iter().any(|e| e.event_type == "output"));
    assert!(events.iter().any(|e| e.event_type == "task_succeeded"));

    handle.abort();
    let _ = handle.await;
}
