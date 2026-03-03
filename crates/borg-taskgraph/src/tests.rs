use anyhow::Result;
use borg_db::BorgDb;
use serde_json::json;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::store::{CreateTaskInput, TaskGraphStore};

async fn test_db() -> Result<BorgDb> {
    let path = format!("/tmp/borg-taskgraph-test-{}.db", Uuid::now_v7());
    let db = BorgDb::open_local(&path).await?;
    db.migrate().await?;
    Ok(db)
}

fn create_input(assignee_agent_id: &str) -> CreateTaskInput {
    CreateTaskInput {
        title: "Task".to_string(),
        description: "desc".to_string(),
        definition_of_done: "dod".to_string(),
        assignee_agent_id: assignee_agent_id.to_string(),
        parent_uri: None,
        blocked_by: vec![],
        references: vec![],
        labels: vec!["initiative:test".to_string()],
    }
}

#[tokio::test]
async fn create_and_get_task_roundtrip() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db);

    let created = store
        .create_task(
            "borg:session:creator",
            "agent:creator",
            create_input("agent:worker"),
        )
        .await?;

    assert_eq!(created.status, "pending");
    assert_eq!(created.assignee_agent_id, "agent:worker");
    assert_eq!(created.reviewer_agent_id, "agent:creator");

    let fetched = store.get_task(&created.uri).await?;
    assert_eq!(fetched.uri, created.uri);
    assert_eq!(fetched.labels, vec!["initiative:test".to_string()]);
    Ok(())
}

#[tokio::test]
async fn blocks_cycle_is_rejected() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db);

    let a = store
        .create_task("borg:session:c1", "agent:c1", create_input("agent:w1"))
        .await?;
    let b = store
        .create_task("borg:session:c2", "agent:c2", create_input("agent:w2"))
        .await?;

    store
        .add_task_blocked_by(&a.reviewer_session_uri, &a.uri, &b.uri)
        .await?;

    let err = store
        .add_task_blocked_by(&b.reviewer_session_uri, &b.uri, &a.uri)
        .await
        .expect_err("cycle should fail");
    assert!(err.to_string().contains("task.cycle_detected"));
    Ok(())
}

#[tokio::test]
async fn review_flow_transitions_pending_review_done() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db);

    let created = store
        .create_task(
            "borg:session:creator",
            "agent:creator",
            create_input("agent:worker"),
        )
        .await?;

    let review = store
        .submit_review(&created.assignee_session_uri, &created.uri)
        .await?;
    assert_eq!(review.status, "review");
    assert!(review.review.submitted_at.is_some());

    let done = store
        .approve_review(&created.reviewer_session_uri, &created.uri)
        .await?;
    assert_eq!(done.status, "done");
    assert!(done.review.approved_at.is_some());
    Ok(())
}

#[tokio::test]
async fn queue_returns_pending_and_doing_when_dependencies_complete() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db);

    let base = store
        .create_task(
            "borg:session:creator-a",
            "agent:creator-a",
            create_input("agent:a"),
        )
        .await?;
    let blocked = store
        .create_task(
            "borg:session:creator-b",
            "agent:creator-b",
            create_input("agent:a"),
        )
        .await?;

    store
        .add_task_blocked_by(&blocked.reviewer_session_uri, &blocked.uri, &base.uri)
        .await?;

    let initial = store.next_task(&blocked.assignee_session_uri, 10).await?;
    assert!(initial.is_empty());

    let _ = store
        .submit_review(&base.assignee_session_uri, &base.uri)
        .await?;
    let _ = store
        .approve_review(&base.reviewer_session_uri, &base.uri)
        .await?;

    let next = store.next_task(&blocked.assignee_session_uri, 10).await?;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].uri, blocked.uri);

    // Doing is also eligible for startup reconcile.
    let _ = store
        .set_task_status(
            &blocked.assignee_session_uri,
            &blocked.uri,
            crate::model::TaskStatus::Doing,
        )
        .await?;
    let in_progress = store
        .reconcile_in_progress(&blocked.assignee_session_uri, 10)
        .await?;
    assert_eq!(in_progress.len(), 1);
    assert_eq!(in_progress[0].uri, blocked.uri);

    Ok(())
}

#[tokio::test]
async fn toolchain_smoke_create_get() -> Result<()> {
    let db = test_db().await?;
    let toolchain = crate::build_taskgraph_toolchain(db.clone())?;

    let create = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: "call-1".to_string(),
            tool_name: "TaskGraph-createTask".to_string(),
            arguments: json!({
                "session_uri": "borg:session:creator",
                "creator_agent_id": "agent:creator",
                "title": "hello",
                "assignee_agent_id": "agent:worker",
                "labels": ["initiative:test"]
            })
            .into(),
        })
        .await?;

    let payload = match create.content {
        borg_agent::ToolResultData::Text(raw) => raw,
        _ => panic!("unexpected output variant"),
    };
    let created: serde_json::Value = serde_json::from_str(&payload)?;
    let uri = created
        .get("task")
        .and_then(|task| task.get("uri"))
        .and_then(serde_json::Value::as_str)
        .expect("uri")
        .to_string();

    let get = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: "call-2".to_string(),
            tool_name: "TaskGraph-getTask".to_string(),
            arguments: json!({ "uri": uri }).into(),
        })
        .await?;

    match get.content {
        borg_agent::ToolResultData::Text(raw) => {
            let value: serde_json::Value = serde_json::from_str(&raw)?;
            assert_eq!(
                value
                    .get("task")
                    .and_then(|task| task.get("title"))
                    .and_then(serde_json::Value::as_str),
                Some("hello")
            );
        }
        _ => panic!("unexpected output variant"),
    }

    Ok(())
}

#[tokio::test]
async fn taskgraph_supervisor_creation_and_lifecycle() -> Result<()> {
    let db = test_db().await?;
    let supervisor = crate::TaskGraphSupervisor::new(db);

    supervisor.start().await;

    supervisor.shutdown().await;

    Ok(())
}

#[tokio::test]
async fn taskgraph_supervisor_dispatches_runnable_tasks() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db.clone());
    let created = store
        .create_task(
            "borg:session:creator",
            "agent:creator",
            create_input("agent:worker"),
        )
        .await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let supervisor = crate::TaskGraphSupervisor::new(db)
        .with_poll_interval(Duration::from_millis(50))
        .with_dispatch(tx);
    supervisor.start().await;

    let dispatch = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("dispatch timeout")
        .expect("dispatch message");
    assert_eq!(dispatch.task_uri, created.uri);
    assert_eq!(dispatch.assignee_session_uri, created.assignee_session_uri);
    Ok(())
}

#[tokio::test]
async fn taskgraph_supervisor_tracks_task_status_changes() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db.clone());

    let task = store
        .create_task(
            "borg:session:creator",
            "agent:creator",
            create_input("agent:worker"),
        )
        .await?;

    let initial_statuses = store.list_all_task_uris().await?;
    assert_eq!(initial_statuses.len(), 1);
    assert_eq!(initial_statuses[0].0, task.uri);
    assert_eq!(initial_statuses[0].1, "pending");

    store
        .set_task_status(
            &task.assignee_session_uri,
            &task.uri,
            crate::model::TaskStatus::Doing,
        )
        .await?;

    let updated_statuses = store.list_all_task_uris().await?;
    assert_eq!(updated_statuses[0].1, "doing");

    Ok(())
}

#[tokio::test]
async fn taskgraph_supervisor_get_task_parent() -> Result<()> {
    let db = test_db().await?;
    let store = TaskGraphStore::new(db.clone());

    let parent = store
        .create_task(
            "borg:session:creator",
            "agent:creator",
            create_input("agent:worker"),
        )
        .await?;

    let child_input = CreateTaskInput {
        title: "Child".to_string(),
        description: "child desc".to_string(),
        definition_of_done: "child dod".to_string(),
        assignee_agent_id: "agent:worker".to_string(),
        parent_uri: Some(parent.uri.clone()),
        blocked_by: vec![],
        references: vec![],
        labels: vec![],
    };

    let child = store
        .create_task("borg:session:creator2", "agent:creator2", child_input)
        .await?;

    let parent_uri = parent.uri.clone();
    let child_parent = store.get_task_parent(&child.uri).await?;
    assert_eq!(child_parent, Some(parent_uri.clone()));

    let parent_parent = store.get_task_parent(&parent_uri).await?;
    assert_eq!(parent_parent, None);

    Ok(())
}
