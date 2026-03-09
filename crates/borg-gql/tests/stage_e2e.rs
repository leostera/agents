use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use borg_codemode::CodeModeRuntime;
use borg_core::{MessageId, MessagePayload, PortId};
use borg_db::BorgDb;
use borg_exec::BorgRuntime;
use borg_fs::BorgFs;
use borg_gql::{BorgHttpServer, HttpPortRequest};
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use serde_json::json;
use std::sync::Arc;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn setup_test_runtime() -> Arc<BorgRuntime> {
    let db_path = format!("/tmp/borg-gql-test-{}.db", Uuid::now_v7());
    let db = BorgDb::open_local(&db_path).await.unwrap();
    db.migrate().await.unwrap();

    let memory_path = format!("/tmp/borg-memory-test-{}.db", Uuid::now_v7());
    let memory = MemoryStore::new(&memory_path, &memory_path).unwrap();
    memory.migrate().await.unwrap();

    let runtime_code = CodeModeRuntime::default();
    let shell_runtime = ShellModeRuntime::new();
    let files = BorgFs::local(db.clone(), std::path::PathBuf::from("/tmp/borg-fs-test"));

    BorgRuntime::new(db, memory, runtime_code, shell_runtime, files)
}

#[tokio::test]
async fn test_stage_http_port_delivery() {
    let runtime = setup_test_runtime().await;
    let supervisor = Arc::new(runtime.supervisor().clone());
    let server = BorgHttpServer::new("127.0.0.1:0".to_string(), runtime.clone(), supervisor);
    let app = server.router();

    let user_key = "test-user-123";
    let text = "Hello Borg!";

    // Send first message
    let req = HttpPortRequest {
        user_key: format!("test://user/{}", user_key),
        text: text.to_string(),
        actor_id: None,
        metadata: Some(json!({ "port": "stage" })),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ports/http")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(res_json["status"], "delivered");
    let message_id_str = res_json["message_id"].as_str().unwrap();
    assert!(message_id_str.starts_with("borg:message:"));

    // Verify actor was created and bound
    let port_id = PortId::from_id("stage");
    let conversation_key = format!("test://user/{}", user_key);
    let binding = runtime
        .db
        .get_port_binding(&port_id, &conversation_key)
        .await
        .unwrap()
        .expect("binding should exist");
    let actor_id = binding.actor_id;
    // Format should be borg:actor:port/stage/test://user/test-user-123
    assert!(actor_id.as_str().contains("port/stage"));

    // Verify message in DB
    let message_id = MessageId::parse(message_id_str).unwrap();
    let msg_record = runtime
        .db
        .get_message(&message_id)
        .await
        .unwrap()
        .expect("message should exist");
    assert_eq!(msg_record.sender_id.as_str(), "borg:port:stage");
    assert_eq!(msg_record.receiver_id.as_str(), actor_id.as_str());

    if let MessagePayload::UserText(p) = msg_record.payload {
        assert!(p.text.contains("\"kind\":\"port_message\""));
        assert!(p.text.contains("Hello Borg!"));
    } else {
        panic!("unexpected payload type");
    }

    // Send second message, should reuse same actor
    let req2 = HttpPortRequest {
        user_key: format!("test://user/{}", user_key),
        text: "Second message".to_string(),
        actor_id: None,
        metadata: None,
    };

    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ports/http")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(res_json2["status"], "delivered");

    // Verify same actor ID is used
    let binding2 = runtime
        .db
        .get_port_binding(&port_id, &conversation_key)
        .await
        .unwrap()
        .expect("binding should exist");
    assert_eq!(binding2.actor_id, actor_id);
}

#[tokio::test]
async fn test_actor_context_window_query() {
    use borg_core::{ActorId, EndpointUri, MessageId, MessagePayload, WorkspaceId};

    let runtime = setup_test_runtime().await;
    let supervisor = Arc::new(runtime.supervisor().clone());
    let server = BorgHttpServer::new("127.0.0.1:0".to_string(), runtime.clone(), supervisor);
    let app = server.router();

    // 1. Create an actor
    let actor_id = ActorId::from_id("test-actor");
    let workspace_id = WorkspaceId::from_id("default");
    runtime
        .db
        .upsert_actor(
            &actor_id,
            &workspace_id,
            "Test Actor",
            "System prompt here",
            "Behavior prompt here",
            "RUNNING",
        )
        .await
        .unwrap();
    runtime
        .db
        .set_actor_model(&actor_id, "gpt-4o")
        .await
        .unwrap();

    // 2. Add some messages
    let user_id = EndpointUri::parse("borg:user:test").unwrap();
    runtime
        .db
        .insert_message(
            &MessageId::new(),
            &workspace_id,
            &user_id,
            &actor_id.clone().into(),
            &MessagePayload::user_text("Hello actor"),
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // 3. Query context window
    let query = json!({
        "query": r#"
            query($id: Uri!) {
                actor(id: $id) {
                    contextWindow {
                        systemPrompt
                        behaviorPrompt
                        orderedMessages {
                            type
                            content
                        }
                    }
                }
            }
        "#,
        "variables": {
            "id": actor_id.as_str()
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/gql")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&query).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        res_json["errors"].is_null(),
        "GraphQL errors: {:?}",
        res_json["errors"]
    );

    let window = &res_json["data"]["actor"]["contextWindow"];
    assert!(
        window["systemPrompt"]
            .as_str()
            .unwrap()
            .contains("Actor messaging protocol:")
    );
    assert_eq!(window["behaviorPrompt"], "System prompt here");

    let messages = window["orderedMessages"].as_array().unwrap();
    assert!(messages.len() >= 2);
    // Index 0 is metadata system message
    assert_eq!(messages[0]["type"], "system");
    assert!(
        messages[0]["content"]
            .as_str()
            .unwrap()
            .contains("BORG_CONTEXT_METADATA_JSON")
    );

    // Index 1 is my user message
    assert_eq!(messages[1]["type"], "user");
    assert_eq!(messages[1]["content"], "Hello actor");
}
