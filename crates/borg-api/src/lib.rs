mod controllers;

use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post, put},
};
use borg_core::{Event, Uri, uri};
use borg_db::BorgDb;
use borg_exec::{ExecEngine, UserMessage};
use borg_ltm::MemoryStore;
use borg_ports::{BORG_SESSION_ID_HEADER, HttpPort, Port, PortMessage, init_http_port, TelegramPort};
use borg_ui::render_dashboard;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::controllers::db::DbController;

const HEALTH_STATUS_OK: &str = "ok";

#[derive(Clone)]
pub(crate) struct AppState {
    db: BorgDb,
    http_port: HttpPort,
    memory: MemoryStore,
}

pub struct BorgApiServer {
    bind: String,
    state: AppState,
    exec: ExecEngine,
}

impl BorgApiServer {
    pub fn new(bind: String, db: BorgDb, exec: ExecEngine, memory: MemoryStore) -> Self {
        Self {
            bind,
            state: AppState {
                db,
                http_port: init_http_port(exec.clone()).expect("failed to initialize http port"),
                memory,
            },
            exec,
        }
    }

    pub async fn run(self) -> Result<()> {
        let telegram_task = start_telegram_port(self.state.db.clone(), self.exec.clone()).await?;
        let router = app_router(self.state);

        let addr: SocketAddr = self.bind.parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!(target: "borg_api", address = %addr, "http api server listening");

        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed waiting for ctrl-c signal");
            info!(target: "borg_api", "received ctrl-c, shutting down");
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await?;

        if let Some(task) = telegram_task {
            task.abort();
        }

        Ok(())
    }
}

fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(ui_dashboard))
        .route("/health", get(health))
        .route("/ports/http", post(ports_http))
        .route("/tasks", get(list_tasks))
        .route("/tasks/:id", get(get_task))
        .route("/tasks/:id/events", get(get_task_events))
        .route("/tasks/:id/output", get(get_task_output))
        .route("/memory/search", get(memory_search))
        .route("/memory/entities/:id", get(get_memory_entity))
        .route("/api/providers", get(DbController::list_providers))
        .route(
            "/api/providers/:provider",
            get(DbController::get_provider)
                .put(DbController::upsert_provider)
                .delete(DbController::delete_provider),
        )
        .route("/api/policies", get(DbController::list_policies))
        .route(
            "/api/policies/:policy_id",
            get(DbController::get_policy)
                .put(DbController::upsert_policy)
                .delete(DbController::delete_policy),
        )
        .route("/api/policies/:policy_id/uses", get(DbController::list_policy_uses))
        .route(
            "/api/policies/:policy_id/uses/:entity_id",
            put(DbController::attach_policy_to_entity).delete(DbController::detach_policy_from_entity),
        )
        .route("/api/agents/specs", get(DbController::list_agent_specs))
        .route(
            "/api/agents/specs/:agent_id",
            get(DbController::get_agent_spec)
                .put(DbController::upsert_agent_spec)
                .delete(DbController::delete_agent_spec),
        )
        .route("/api/users", get(DbController::list_users).post(DbController::upsert_user))
        .route(
            "/api/users/:user_key",
            get(DbController::get_user)
                .patch(DbController::patch_user)
                .delete(DbController::delete_user),
        )
        .route("/api/sessions", get(DbController::list_sessions).post(DbController::upsert_session))
        .route(
            "/api/sessions/:session_id",
            get(DbController::get_session)
                .patch(DbController::patch_session)
                .delete(DbController::delete_session),
        )
        .route(
            "/api/sessions/:session_id/messages",
            get(DbController::list_session_messages)
                .post(DbController::append_session_message)
                .delete(DbController::clear_session_messages),
        )
        .route(
            "/api/sessions/:session_id/messages/:message_index",
            get(DbController::get_session_message)
                .patch(DbController::patch_session_message)
                .delete(DbController::delete_session_message),
        )
        .route("/api/ports/:port/settings", get(DbController::list_port_settings))
        .route(
            "/api/ports/:port/settings/:key",
            get(DbController::get_port_setting)
                .put(DbController::upsert_port_setting)
                .delete(DbController::delete_port_setting),
        )
        .route("/api/ports/:port/bindings", get(DbController::list_port_bindings))
        .route(
            "/api/ports/:port/bindings/:conversation_key",
            get(DbController::get_port_binding)
                .put(DbController::upsert_port_binding)
                .delete(DbController::delete_port_binding),
        )
        .route(
            "/api/ports/:port/sessions/:session_id/context",
            get(DbController::get_port_session_context)
                .put(DbController::upsert_port_session_context)
                .delete(DbController::delete_port_session_context),
        )
        .route("/api/sessions/:session_id/context", get(DbController::get_any_port_session_context))
        .with_state(state)
}

async fn start_telegram_port(db: BorgDb, exec: ExecEngine) -> Result<Option<JoinHandle<()>>> {
    let token = db.get_port_setting("telegram", "bot_token").await?;
    let Some(token) = token else {
        info!(target: "borg_api", "telegram port disabled (no token configured)");
        return Ok(None);
    };

    let token = token.trim().to_string();
    if token.is_empty() {
        info!(target: "borg_api", "telegram port disabled (empty token)");
        return Ok(None);
    }

    let telegram_port = TelegramPort::new(exec, token)?;
    let task = tokio::spawn(async move {
        if let Err(err) = telegram_port.run().await {
            error!(target: "borg_api", error = %err, "telegram port terminated");
        }
    });

    info!(target: "borg_api", "telegram port enabled");
    Ok(Some(task))
}

#[derive(Deserialize)]
struct TasksQuery {
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct MemorySearchQuery {
    q: String,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Serialize)]
struct ApiValidationError {
    error: String,
    details: Vec<ApiFieldError>,
}

#[derive(Serialize)]
struct ApiFieldError {
    field: String,
    message: String,
}

#[derive(Deserialize)]
struct HttpPortRequest {
    user_key: String,
    text: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
}

async fn health() -> impl IntoResponse {
    debug!(target: "borg_api", "health endpoint called");
    Json(json!({ "status": HEALTH_STATUS_OK }))
}

async fn ui_dashboard(State(state): State<AppState>) -> impl IntoResponse {
    debug!(target: "borg_api", "ui dashboard endpoint called");
    let tasks_count = state
        .db
        .list_tasks(None, 10_000)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let entities_count = state
        .memory
        .search("movie", None, 10_000)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    Html(render_dashboard(tasks_count, entities_count))
}

async fn ports_http(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<HttpPortRequest>,
) -> impl IntoResponse {
    let payload = match validate_port_request(payload) {
        Ok(value) => value,
        Err(err) => return err,
    };
    info!(target: "borg_api", user_key = %payload.user_key, text = payload.text, "received HTTP port event");
    let inbound = PortMessage::from_http(&headers, payload);
    let mut messages = state.http_port.handle_messages(vec![inbound]).await;
    match messages.pop() {
        Some(message) if message.error.is_none() => {
            let mut response = (
                StatusCode::OK,
                Json(json!({
                    "task_id": message.task_id,
                    "session_id": message.session_id,
                    "reply": message.reply
                })),
            )
                .into_response();
            if let Some(session_id) = message.session_id {
                if let Ok(value) = session_id.to_string().parse() {
                    response.headers_mut().insert(BORG_SESSION_ID_HEADER, value);
                }
            }
            response
        }
        Some(message) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            message
                .error
                .unwrap_or_else(|| "port adapter failed".to_string()),
        ),
        None => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "empty port response".to_string(),
        ),
    }
}

fn validate_port_request(
    payload: HttpPortRequest,
) -> Result<UserMessage, axum::response::Response> {
    let mut details = Vec::new();
    let user_key = match Uri::parse(&payload.user_key) {
        Ok(value) => Some(value),
        Err(_) => {
            details.push(ApiFieldError {
                field: "user_key".to_string(),
                message: "must be a valid URI".to_string(),
            });
            None
        }
    };

    let session_id = match payload.session_id {
        Some(raw) => match Uri::parse(&raw) {
            Ok(value) => Some(value),
            Err(_) => {
                details.push(ApiFieldError {
                    field: "session_id".to_string(),
                    message: "must be a valid URI".to_string(),
                });
                None
            }
        },
        None => None,
    };

    let agent_id = match payload.agent_id {
        Some(raw) => match Uri::parse(&raw) {
            Ok(value) => Some(value),
            Err(_) => {
                details.push(ApiFieldError {
                    field: "agent_id".to_string(),
                    message: "must be a valid URI".to_string(),
                });
                None
            }
        },
        None => None,
    };

    if !details.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiValidationError {
                error: "invalid request".to_string(),
                details,
            }),
        )
            .into_response());
    }

    Ok(UserMessage {
        user_key: user_key.expect("validated user_key"),
        text: payload.text,
        session_id,
        agent_id,
        metadata: payload
            .metadata
            .unwrap_or(Value::Object(Default::default())),
    })
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    debug!(target: "borg_api", status = ?query.status, limit, "listing tasks endpoint");
    match state.db.list_tasks(query.status, limit).await {
        Ok(tasks) => (StatusCode::OK, Json(json!({ "tasks": tasks }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_task(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_api", task_id, "get task endpoint");
    let Ok(task_id) = Uri::parse(&task_id) else {
        return api_error(StatusCode::BAD_REQUEST, "invalid task id".to_string());
    };
    match state.db.get_task(&task_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "task not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_task_events(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_api", task_id, "get task events endpoint");
    let Ok(task_id) = Uri::parse(&task_id) else {
        return api_error(StatusCode::BAD_REQUEST, "invalid task id".to_string());
    };
    match state.db.get_task_events(&task_id).await {
        Ok(events) => (StatusCode::OK, Json(json!({ "events": events }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_task_output(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_api", task_id, "get task output endpoint");
    let Ok(task_id) = Uri::parse(&task_id) else {
        return api_error(StatusCode::BAD_REQUEST, "invalid task id".to_string());
    };

    let events = match state.db.get_task_events(&task_id).await {
        Ok(events) => events,
        Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };

    let agent_output_type = uri!("borg", "agent", "output");
    let task_succeeded_type = uri!("borg", "task", "succeeded");
    for event in events.iter().rev() {
        if event.event_type != agent_output_type && event.event_type != task_succeeded_type {
            continue;
        }
        if let Ok(parsed) = serde_json::from_value::<Event>(event.payload.clone()) {
            match parsed {
                Event::AgentOutput { message, .. } => {
                    return (StatusCode::OK, Json(json!({ "message": message }))).into_response();
                }
                Event::TaskSucceeded { message, .. } => {
                    return (StatusCode::OK, Json(json!({ "message": message }))).into_response();
                }
                _ => {}
            }
        }
    }

    api_error(
        StatusCode::NOT_FOUND,
        "task output not available yet".to_string(),
    )
}

async fn memory_search(
    State(state): State<AppState>,
    Query(query): Query<MemorySearchQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(25);
    debug!(target: "borg_api", q = query.q, entity_type = ?query.entity_type, limit, "memory search endpoint");

    match state
        .memory
        .search(&query.q, query.entity_type.as_deref(), limit)
        .await
    {
        Ok(entities) => (StatusCode::OK, Json(json!({ "entities": entities }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_memory_entity(
    State(state): State<AppState>,
    AxumPath(entity_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_api", entity_id, "get memory entity endpoint");
    match state.memory.get_entity(&entity_id).await {
        Ok(Some(entity)) => (StatusCode::OK, Json(json!({ "entity": entity }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "entity not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

fn api_error(status: StatusCode, error: String) -> axum::response::Response {
    (status, Json(ApiError { error })).into_response()
}

#[cfg(test)]
mod tests {
    use super::{AppState, HttpPortRequest, app_router, validate_port_request};
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode};
    use borg_core::Uri;
    use borg_db::BorgDb;
    use borg_exec::ExecEngine;
    use borg_ltm::MemoryStore;
    use borg_ports::init_http_port;
    use borg_rt::CodeModeRuntime;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    fn test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let pid = std::process::id();
        std::env::temp_dir().join(format!("borg-api-{name}-{pid}-{nanos}"))
    }

    async fn test_app(name: &str) -> axum::Router {
        let root = test_root(name);
        let db_path = root.join("config.db");
        let memory_path = root.join("ltm");
        let search_path = root.join("search");
        std::fs::create_dir_all(&memory_path).expect("create memory path");
        std::fs::create_dir_all(&search_path).expect("create search path");

        let db = BorgDb::open_local(db_path.to_string_lossy().as_ref())
            .await
            .expect("open local db");
        db.migrate().await.expect("migrate db");

        let memory = MemoryStore::new(&memory_path, &search_path).expect("new memory store");
        memory.migrate().await.expect("migrate memory");

        let exec = ExecEngine::new(
            db.clone(),
            memory.clone(),
            CodeModeRuntime::default(),
            Uri::parse("borg:worker:test").expect("worker uri"),
        );
        let http_port = init_http_port(exec).expect("init http port");
        let state = AppState {
            db,
            http_port,
            memory,
        };
        app_router(state)
    }

    async fn request_json(
        app: &axum::Router,
        method: Method,
        path: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("build request"),
            )
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let parsed = if bytes.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&bytes).expect("json response")
        };
        (status, parsed)
    }

    async fn request_no_body(app: &axum::Router, method: Method, path: &str) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let parsed = if bytes.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&bytes).expect("json response")
        };
        (status, parsed)
    }

    #[test]
    fn validate_port_request_rejects_invalid_uri_fields() {
        let request = HttpPortRequest {
            user_key: "not a uri".to_string(),
            text: "hello".to_string(),
            session_id: Some("bad session".to_string()),
            agent_id: Some("bad agent".to_string()),
            metadata: Some(json!({})),
        };
        assert!(validate_port_request(request).is_err());
    }

    #[test]
    fn validate_port_request_accepts_valid_uri_fields() {
        let request = HttpPortRequest {
            user_key: "borg:user:test".to_string(),
            text: "hello".to_string(),
            session_id: Some("borg:session:123".to_string()),
            agent_id: Some("borg:agent:default".to_string()),
            metadata: Some(json!({"a":"b"})),
        };
        let parsed = validate_port_request(request).unwrap();
        assert_eq!(parsed.user_key.as_str(), "borg:user:test");
        assert_eq!(parsed.session_id.unwrap().as_str(), "borg:session:123");
        assert_eq!(parsed.agent_id.unwrap().as_str(), "borg:agent:default");
    }

    #[tokio::test]
    async fn providers_crud_endpoints_work() {
        let app = test_app("providers").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/providers/openai",
            json!({"api_key":"sk-test"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "openai");

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["providers"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn policies_and_policy_uses_crud_endpoints_work() {
        let app = test_app("policies").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:session-read",
            json!({"policy":{"effect":"allow","actions":["session.read"]}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/policies/borg:policy:session-read").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["policy"]["policy"]["effect"], "allow");

        let (status, body) = request_no_body(&app, Method::GET, "/api/policies").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["policies"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:session-read/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/policies/borg:policy:session-read/uses",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["uses"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/policies/borg:policy:session-read/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/policies/borg:policy:session-read").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn agent_specs_crud_endpoints_work() {
        let app = test_app("agent-specs").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/agents/specs/borg:agent:default",
            json!({
                "model":"gpt-4o-mini",
                "system_prompt":"you are borg",
                "tools":[{"name":"search"}]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["agent_spec"]["model"], "gpt-4o-mini");

        let (status, body) = request_no_body(&app, Method::GET, "/api/agents/specs").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["agent_specs"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn users_crud_endpoints_work() {
        let app = test_app("users").await;
        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/users",
            json!({"user_key":"borg:user:test","profile":{"name":"Test"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/users/borg:user:test").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["profile"]["name"], "Test");

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/users/borg:user:test",
            json!({"profile":{"name":"Updated"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/users").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["users"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/users/borg:user:test").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_settings_crud_endpoints_work() {
        let app = test_app("port-settings").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/telegram/settings/bot_token",
            json!({"value":"123:abc"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/settings/bot_token",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["value"], "123:abc");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["settings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/telegram/settings/bot_token",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_bindings_and_context_endpoints_work() {
        let app = test_app("port-bindings-context").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/telegram/bindings/borg:user:chat1",
            json!({
                "session_id":"borg:session:s1",
                "agent_id":"borg:agent:default"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/bindings/borg:user:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["binding"]["session_id"], "borg:session:s1");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/bindings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["bindings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/telegram/sessions/borg:session:s1/context",
            json!({"ctx":{"chat_id":"123"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/sessions/borg:session:s1/context",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ctx"]["chat_id"], "123");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/sessions/borg:session:s1/context").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["port"], "telegram");

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/telegram/sessions/borg:session:s1/context",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/telegram/bindings/borg:user:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn sessions_and_messages_crud_endpoints_work() {
        let app = test_app("sessions").await;
        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/sessions",
            json!({
                "session_id":"borg:session:test",
                "user_key":"borg:user:test",
                "port":"http",
                "root_task_id":"borg:task:root",
                "state":{"mode":"chat"}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/sessions/borg:session:test").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["port"], "http");

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/sessions/borg:session:test",
            json!({"state":{"mode":"agent"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/sessions/borg:session:test/messages",
            json!({"payload":{"role":"user","content":"hello"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:test/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"]["message_index"], 0);

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/sessions/borg:session:test/messages/0",
            json!({"payload":{"role":"user","content":"updated"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:test/messages?from=0&limit=10",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/sessions/borg:session:test/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/sessions/borg:session:test/messages",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/sessions/borg:session:test").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn providers_negative_paths() {
        let app = test_app("providers-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/providers/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/providers/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn policies_negative_paths() {
        let app = test_app("policies-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/policies/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/policies/borg:policy:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:missing/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/not-a-uri/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn agents_negative_paths() {
        let app = test_app("agents-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/agents/specs/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/agents/specs/borg:agent:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn users_negative_paths() {
        let app = test_app("users-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/users/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/users/borg:user:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sessions_negative_paths() {
        let app = test_app("sessions-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/sessions/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/sessions/borg:session:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:missing/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ports_negative_paths() {
        let app = test_app("ports-negative").await;
        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/settings/missing",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/bindings/not-a-uri",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/sessions/not-a-uri/context",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:missing/context",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
