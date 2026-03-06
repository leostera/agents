use std::time::Duration;

use async_graphql::futures_util::StreamExt;
use borg_core::Uri;
use borg_db::BorgDb;
use borg_memory::{FactArity, FactValue, MemoryStore, Uri as MemoryUri};
use serde_json::json;

use super::*;

fn tmp_path(prefix: &str, ext: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("{prefix}-{}.{}", uuid::Uuid::new_v4(), ext));
    path.to_string_lossy().to_string()
}

async fn test_schema() -> anyhow::Result<BorgGqlSchema> {
    let db_path = tmp_path("borg-gql-test-db", "db");
    let memory_path = tmp_path("borg-gql-test-memory", "db");
    let search_path = tmp_path("borg-gql-test-search", "db");

    let db = BorgDb::open_local(&db_path).await?;
    db.migrate().await?;

    let memory = MemoryStore::new(&memory_path, &search_path)?;
    memory.migrate().await?;

    Ok(BorgGqlServer::new(db, memory).schema())
}

#[tokio::test]
async fn actor_workspace_query_roundtrip() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let data = schema.data::<BorgGqlData>().expect("gql data").clone();

    let actor_id = Uri::from_parts("borg", "actor", Some("a1"))?;
    let session_id = Uri::from_parts("borg", "session", Some("s1"))?;
    let port_id = Uri::from_parts("borg", "port", Some("http"))?;

    data.db
        .upsert_actor(&actor_id, "actor", "prompt", "RUNNING")
        .await?;
    data.db.upsert_session(&session_id, &port_id).await?;
    data.db
        .append_session_message(&session_id, &json!({"type":"user","content":"hello"}), None)
        .await?;
    data.db
        .enqueue_actor_message(
            &actor_id,
            Some(&session_id),
            &json!({"source":"tests"}),
            None,
            None,
        )
        .await?;

    let query = r#"
          query($id: Uri!) {
            actor(id: $id) {
              id
              name
              sessions(first: 5) {
                edges {
                  node {
                    id
                    messages(first: 5) {
                      edges {
                        node {
                          id
                          messageType
                          role
                          text
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        "#;

    let response = schema
        .execute(
            async_graphql::Request::new(query)
                .variables(async_graphql::Variables::from_json(json!({"id": actor_id}))),
        )
        .await;

    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;
    assert_eq!(data["actor"]["name"], "actor");
    assert_eq!(
        data["actor"]["sessions"]["edges"][0]["node"]["messages"]["edges"][0]["node"]["text"],
        "hello"
    );

    Ok(())
}

#[tokio::test]
async fn upsert_and_list_provider_via_graphql() -> anyhow::Result<()> {
    let schema = test_schema().await?;

    let mutation = r#"
          mutation {
            upsertProvider(input: {
              provider: "openai"
              providerKind: "openai"
              apiKey: "sk-test"
              enabled: true
              defaultTextModel: "gpt-4.1-mini"
            }) {
              provider
              providerKind
              enabled
              defaultTextModel
            }
          }
        "#;

    let response = schema.execute(mutation).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);

    let query = r#"
          query {
            providers(first: 10) {
              edges {
                node {
                  provider
                  providerKind
                }
              }
            }
          }
        "#;
    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;
    assert_eq!(data["providers"]["edges"][0]["node"]["provider"], "openai");

    Ok(())
}

#[tokio::test]
async fn task_creation_and_status_transition() -> anyhow::Result<()> {
    let schema = test_schema().await?;

    let session_uri = Uri::from_parts("borg", "session", Some("task-session"))?;
    let creator = Uri::from_parts("borg", "actor", Some("creator"))?;
    let assignee = Uri::from_parts("borg", "actor", Some("assignee"))?;

    let create_mutation = r#"
          mutation CreateTask($session: Uri!, $creator: Uri!, $assignee: Uri!) {
            createTask(input: {
              sessionUri: $session
              creatorActorId: $creator
              assigneeActorId: $assignee
              title: "Ship borg-gql"
              description: "Implement gql"
              definitionOfDone: "tests pass"
            }) {
              id
              title
              status
              assigneeSessionId
            }
          }
        "#;

    let create_response = schema
        .execute(async_graphql::Request::new(create_mutation).variables(
            async_graphql::Variables::from_json(json!({
                "session": session_uri,
                "creator": creator,
                "assignee": assignee,
            })),
        ))
        .await;

    assert!(
        create_response.errors.is_empty(),
        "{:#?}",
        create_response.errors
    );

    let created = create_response.data.into_json()?;
    let created_id = created["createTask"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing task id"))?
        .to_string();
    let assignee_session = created["createTask"]["assigneeSessionId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing assignee session"))?
        .to_string();

    let status_mutation = r#"
          mutation SetTask($task: Uri!, $session: Uri!) {
            setTaskStatus(input: {
              taskId: $task
              sessionUri: $session
              status: DOING
            }) {
              id
              status
            }
          }
        "#;

    let status_response = schema
        .execute(async_graphql::Request::new(status_mutation).variables(
            async_graphql::Variables::from_json(json!({
                "task": created_id,
                "session": assignee_session,
            })),
        ))
        .await;

    assert!(
        status_response.errors.is_empty(),
        "{:#?}",
        status_response.errors
    );

    let status = status_response.data.into_json()?;
    assert_eq!(status["setTaskStatus"]["status"], "DOING");
    Ok(())
}

#[tokio::test]
async fn memory_entities_and_facts_are_typed() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let data = schema.data::<BorgGqlData>().expect("gql data").clone();

    let source = MemoryUri::from_parts("borg", "source", Some("tests"))?;
    let entity = MemoryUri::from_parts("borg", "entity", Some("alice"))?;
    let field = MemoryUri::from_parts("borg", "field", Some("name"))?;

    data.memory
        .state_facts(vec![borg_memory::FactInput {
            source,
            entity,
            field,
            arity: FactArity::One,
            value: FactValue::Text("Alice".to_string()),
        }])
        .await?;

    let query = r#"
          query {
            memoryFacts(first: 10) {
              edges {
                node {
                  arity
                  value { kind text }
                }
              }
            }
          }
        "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;
    assert_eq!(
        data["memoryFacts"]["edges"][0]["node"]["value"]["text"],
        "Alice"
    );

    Ok(())
}

#[tokio::test]
async fn schema_has_node_interface_and_core_types() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let sdl = schema.sdl();
    assert!(sdl.contains("interface Node"));
    assert!(sdl.contains("type Actor"));
    assert!(sdl.contains("type Session"));
    assert!(sdl.contains("type Task"));
    assert!(sdl.contains("scalar Uri"));
    Ok(())
}

#[tokio::test]
async fn status_fields_and_inputs_use_enum_types() -> anyhow::Result<()> {
    let schema = test_schema().await?;

    let query = r#"
          query {
            actor: __type(name: "Actor") { fields { name type { kind name ofType { kind name ofType { kind name } } } } }
            app: __type(name: "App") { fields { name type { kind name ofType { kind name ofType { kind name } } } } }
            appCapability: __type(name: "AppCapability") { fields { name type { kind name ofType { kind name ofType { kind name } } } } }
            appConnection: __type(name: "AppConnection") { fields { name type { kind name ofType { kind name ofType { kind name } } } } }
            scheduleJob: __type(name: "ScheduleJob") { fields { name type { kind name ofType { kind name ofType { kind name } } } } }
            upsertActorInput: __type(name: "UpsertActorInput") { inputFields { name type { kind name ofType { kind name ofType { kind name } } } } }
            upsertAppInput: __type(name: "UpsertAppInput") { inputFields { name type { kind name ofType { kind name ofType { kind name } } } } }
            upsertAppCapabilityInput: __type(name: "UpsertAppCapabilityInput") { inputFields { name type { kind name ofType { kind name ofType { kind name } } } } }
            upsertAppConnectionInput: __type(name: "UpsertAppConnectionInput") { inputFields { name type { kind name ofType { kind name ofType { kind name } } } } }
            queryRoot: __type(name: "QueryRoot") {
              fields {
                name
                args { name type { kind name ofType { kind name ofType { kind name } } } }
              }
            }
          }
        "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;

    fn leaf_type_name(ty: &serde_json::Value) -> Option<&str> {
        let kind = ty.get("kind")?.as_str()?;
        match kind {
            "NON_NULL" | "LIST" => leaf_type_name(ty.get("ofType")?),
            _ => ty.get("name")?.as_str(),
        }
    }

    fn field_type_name(type_json: &serde_json::Value, field_name: &str) -> anyhow::Result<String> {
        let fields = type_json["fields"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing fields"))?;
        let field = fields
            .iter()
            .find(|f| f["name"].as_str() == Some(field_name))
            .ok_or_else(|| anyhow::anyhow!("missing field {field_name}"))?;
        leaf_type_name(&field["type"])
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("missing type for field {field_name}"))
    }

    fn input_field_type_name(
        type_json: &serde_json::Value,
        field_name: &str,
    ) -> anyhow::Result<String> {
        let fields = type_json["inputFields"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing inputFields"))?;
        let field = fields
            .iter()
            .find(|f| f["name"].as_str() == Some(field_name))
            .ok_or_else(|| anyhow::anyhow!("missing input field {field_name}"))?;
        leaf_type_name(&field["type"])
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("missing type for input field {field_name}"))
    }

    assert_eq!(
        field_type_name(&data["actor"], "status")?,
        "ActorStatusValue"
    );
    assert_eq!(field_type_name(&data["app"], "status")?, "AppStatusValue");
    assert_eq!(
        field_type_name(&data["appCapability"], "status")?,
        "AppCapabilityStatusValue"
    );
    assert_eq!(
        field_type_name(&data["appConnection"], "status")?,
        "AppConnectionStatusValue"
    );
    assert_eq!(
        field_type_name(&data["scheduleJob"], "status")?,
        "ScheduleJobStatusValue"
    );

    assert_eq!(
        input_field_type_name(&data["upsertActorInput"], "status")?,
        "ActorStatusValue"
    );
    assert_eq!(
        input_field_type_name(&data["upsertAppInput"], "status")?,
        "AppStatusValue"
    );
    assert_eq!(
        input_field_type_name(&data["upsertAppCapabilityInput"], "status")?,
        "AppCapabilityStatusValue"
    );
    assert_eq!(
        input_field_type_name(&data["upsertAppConnectionInput"], "status")?,
        "AppConnectionStatusValue"
    );

    let query_fields = data["queryRoot"]["fields"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing QueryRoot fields"))?;
    let schedule_jobs_field = query_fields
        .iter()
        .find(|field| field["name"].as_str() == Some("scheduleJobs"))
        .ok_or_else(|| anyhow::anyhow!("missing QueryRoot.scheduleJobs"))?;
    let status_arg = schedule_jobs_field["args"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing scheduleJobs args"))?
        .iter()
        .find(|arg| arg["name"].as_str() == Some("status"))
        .ok_or_else(|| anyhow::anyhow!("missing scheduleJobs.status arg"))?;
    let status_arg_type = leaf_type_name(&status_arg["type"])
        .ok_or_else(|| anyhow::anyhow!("missing scheduleJobs.status type"))?;
    assert_eq!(status_arg_type, "ScheduleJobStatusValue");

    Ok(())
}

#[tokio::test]
async fn root_fields_are_documented_with_examples() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let query = r#"
          query {
            queryRoot: __type(name: "QueryRoot") {
              description
              fields { name description }
            }
            mutationRoot: __type(name: "MutationRoot") {
              description
              fields { name description }
            }
          }
        "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;

    for root_key in ["queryRoot", "mutationRoot"] {
        let root_desc = data[root_key]["description"].as_str().unwrap_or_default();
        assert!(
            root_desc.contains("Usage notes:"),
            "{root_key} missing usage notes"
        );
        assert!(root_desc.contains("Example:"), "{root_key} missing example");

        let fields = data[root_key]["fields"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing fields for {root_key}"))?;

        for field in fields {
            let name = field["name"].as_str().unwrap_or("<unknown>");
            let description = field["description"].as_str().unwrap_or_default();
            assert!(
                !description.is_empty(),
                "{root_key}.{name} is missing a description"
            );
            assert!(
                description.contains("Example:"),
                "{root_key}.{name} is missing an example"
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn core_object_types_have_usage_docs() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let query = r#"
          query {
            __schema {
              types {
                name
                description
              }
            }
          }
        "#;

    let response = schema.execute(query).await;
    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let data = response.data.into_json()?;

    let docs = data["__schema"]["types"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing __schema.types"))?
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("name")?.as_str()?.to_string(),
                entry
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let required = [
        "Actor",
        "Session",
        "SessionMessage",
        "Port",
        "PortBinding",
        "PortActorBinding",
        "Provider",
        "App",
        "AppCapability",
        "AppConnection",
        "AppSecret",
        "ScheduleJob",
        "ScheduleJobRun",
        "Task",
        "TaskComment",
        "TaskEvent",
        "MemoryEntity",
        "MemoryFact",
    ];

    for name in required {
        let description = docs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("missing type {name}"))?;
        assert!(!description.is_empty(), "{name} missing description");
        assert!(
            description.contains("Example:"),
            "{name} description missing example"
        );
    }

    Ok(())
}

#[tokio::test]
async fn subscription_session_chat_streams_new_messages() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let data = schema.data::<BorgGqlData>().expect("gql data").clone();

    let session_id = Uri::from_parts("borg", "session", Some("sub-chat"))?;
    let port_id = Uri::from_parts("borg", "port", Some("http"))?;
    data.db.upsert_session(&session_id, &port_id).await?;

    let request = async_graphql::Request::new(
        r#"
              subscription($session: Uri!) {
                sessionChat(sessionId: $session, pollIntervalMs: 100) {
                  id
                  messageType
                  role
                  text
                }
              }
            "#,
    )
    .variables(async_graphql::Variables::from_json(
        json!({ "session": session_id }),
    ));

    let mut stream = schema.execute_stream(request);

    data.db
        .append_session_message(
            &session_id,
            &json!({"type":"assistant","role":"assistant","content":"hello from subscription"}),
            None,
        )
        .await?;

    let response = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for subscription event"))?
        .ok_or_else(|| anyhow::anyhow!("subscription ended unexpectedly"))?;

    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let payload = response.data.into_json()?;
    assert_eq!(payload["sessionChat"]["messageType"], "assistant");
    assert_eq!(payload["sessionChat"]["role"], "assistant");
    assert_eq!(payload["sessionChat"]["text"], "hello from subscription");
    Ok(())
}

#[tokio::test]
async fn subscription_notifications_filter_user_messages_by_default() -> anyhow::Result<()> {
    let schema = test_schema().await?;
    let data = schema.data::<BorgGqlData>().expect("gql data").clone();

    let session_id = Uri::from_parts("borg", "session", Some("sub-notifications"))?;
    let port_id = Uri::from_parts("borg", "port", Some("http"))?;
    data.db.upsert_session(&session_id, &port_id).await?;

    let request = async_graphql::Request::new(
        r#"
              subscription($session: Uri!) {
                sessionNotifications(sessionId: $session, pollIntervalMs: 100) {
                  kind
                  messageType
                  role
                  text
                }
              }
            "#,
    )
    .variables(async_graphql::Variables::from_json(
        json!({ "session": session_id }),
    ));

    let mut stream = schema.execute_stream(request);

    data.db
        .append_session_message(
            &session_id,
            &json!({"type":"user","role":"user","content":"user message"}),
            None,
        )
        .await?;
    data.db
        .append_session_message(
            &session_id,
            &json!({"type":"assistant","role":"assistant","content":"assistant notification"}),
            None,
        )
        .await?;

    let response = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for notification event"))?
        .ok_or_else(|| anyhow::anyhow!("subscription ended unexpectedly"))?;

    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    let payload = response.data.into_json()?;
    assert_eq!(payload["sessionNotifications"]["kind"], "ASSISTANT_REPLY");
    assert_eq!(payload["sessionNotifications"]["messageType"], "assistant");
    assert_eq!(payload["sessionNotifications"]["role"], "assistant");
    assert_eq!(
        payload["sessionNotifications"]["text"],
        "assistant notification"
    );
    Ok(())
}

#[test]
fn static_schema_snapshot_is_generated() {
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema.graphql");
    let schema = std::fs::read_to_string(schema_path).expect("generated schema.graphql");
    assert!(schema.contains("interface Node"));
    assert!(schema.contains("type MutationRoot"));
    assert!(schema.contains("type SubscriptionRoot"));
}

#[tokio::test]
async fn introspection_query_supports_graphiql_depth() -> anyhow::Result<()> {
    let schema = test_schema().await?;

    let response = schema
        .execute(
            r#"
            query {
              __type(name: "Actor") {
                fields {
                  type {
                    ofType {
                      ofType {
                        ofType {
                          ofType {
                            ofType {
                              ofType {
                                ofType {
                                  ofType {
                                    ofType {
                                      ofType {
                                        ofType {
                                          ofType {
                                            ofType {
                                              ofType {
                                                name
                                                kind
                                              }
                                            }
                                          }
                                        }
                                      }
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            "#,
        )
        .await;

    assert!(response.errors.is_empty(), "{:#?}", response.errors);
    Ok(())
}
