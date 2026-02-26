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

const HEALTH_STATUS_OK: &str = "ok";

#[derive(Clone)]
struct AppState {
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
        .route("/api/providers", get(api_list_providers))
        .route(
            "/api/providers/:provider",
            get(api_get_provider)
                .put(api_upsert_provider)
                .delete(api_delete_provider),
        )
        .route("/api/policies", get(api_list_policies))
        .route(
            "/api/policies/:policy_id",
            get(api_get_policy)
                .put(api_upsert_policy)
                .delete(api_delete_policy),
        )
        .route("/api/policies/:policy_id/uses", get(api_list_policy_uses))
        .route(
            "/api/policies/:policy_id/uses/:entity_id",
            put(api_attach_policy_to_entity).delete(api_detach_policy_from_entity),
        )
        .route("/api/agents/specs", get(api_list_agent_specs))
        .route(
            "/api/agents/specs/:agent_id",
            get(api_get_agent_spec)
                .put(api_upsert_agent_spec)
                .delete(api_delete_agent_spec),
        )
        .route("/api/users", get(api_list_users).post(api_upsert_user))
        .route(
            "/api/users/:user_key",
            get(api_get_user)
                .patch(api_patch_user)
                .delete(api_delete_user),
        )
        .route("/api/sessions", get(api_list_sessions).post(api_upsert_session))
        .route(
            "/api/sessions/:session_id",
            get(api_get_session)
                .patch(api_patch_session)
                .delete(api_delete_session),
        )
        .route(
            "/api/sessions/:session_id/messages",
            get(api_list_session_messages)
                .post(api_append_session_message)
                .delete(api_clear_session_messages),
        )
        .route(
            "/api/sessions/:session_id/messages/:message_index",
            get(api_get_session_message)
                .patch(api_patch_session_message)
                .delete(api_delete_session_message),
        )
        .route("/api/ports/:port/settings", get(api_list_port_settings))
        .route(
            "/api/ports/:port/settings/:key",
            get(api_get_port_setting)
                .put(api_upsert_port_setting)
                .delete(api_delete_port_setting),
        )
        .route("/api/ports/:port/bindings", get(api_list_port_bindings))
        .route(
            "/api/ports/:port/bindings/:conversation_key",
            get(api_get_port_binding)
                .put(api_upsert_port_binding)
                .delete(api_delete_port_binding),
        )
        .route(
            "/api/ports/:port/sessions/:session_id/context",
            get(api_get_port_session_context)
                .put(api_upsert_port_session_context)
                .delete(api_delete_port_session_context),
        )
        .route("/api/sessions/:session_id/context", get(api_get_any_port_session_context))
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

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SessionsQuery {
    limit: Option<usize>,
    port: Option<String>,
    user_key: Option<String>,
}

#[derive(Deserialize)]
struct PortSettingsQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct PortBindingsQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct UpsertProviderRequest {
    api_key: String,
}

#[derive(Deserialize)]
struct UpsertPolicyRequest {
    policy: Value,
}

#[derive(Deserialize)]
struct UpsertAgentSpecRequest {
    model: String,
    system_prompt: String,
    tools: Value,
}

#[derive(Deserialize)]
struct UpsertUserRequest {
    user_key: String,
    profile: Value,
}

#[derive(Deserialize)]
struct PatchUserRequest {
    profile: Value,
}

#[derive(Deserialize)]
struct UpsertSessionRequest {
    session_id: String,
    user_key: String,
    port: String,
    root_task_id: String,
    state: Value,
}

#[derive(Deserialize)]
struct PatchSessionRequest {
    user_key: Option<String>,
    port: Option<String>,
    root_task_id: Option<String>,
    state: Option<Value>,
}

#[derive(Deserialize)]
struct SessionMessagesQuery {
    from: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct UpsertSessionMessageRequest {
    payload: Value,
}

#[derive(Deserialize)]
struct UpsertPortSettingRequest {
    value: String,
}

#[derive(Deserialize)]
struct UpsertPortBindingRequest {
    session_id: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Deserialize)]
struct UpsertPortSessionContextRequest {
    ctx: Value,
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

fn parse_uri_field(field: &str, raw: &str) -> Result<Uri, axum::response::Response> {
    Uri::parse(raw).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiValidationError {
                error: "invalid request".to_string(),
                details: vec![ApiFieldError {
                    field: field.to_string(),
                    message: "must be a valid URI".to_string(),
                }],
            }),
        )
            .into_response()
    })
}

async fn api_list_providers(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    match state.db.list_providers(limit).await {
        Ok(providers) => (StatusCode::OK, Json(json!({ "providers": providers }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_provider(
    State(state): State<AppState>,
    AxumPath(provider): AxumPath<String>,
) -> impl IntoResponse {
    match state.db.get_provider(&provider).await {
        Ok(Some(found)) => (StatusCode::OK, Json(json!({ "provider": found }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "provider not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_provider(
    State(state): State<AppState>,
    AxumPath(provider): AxumPath<String>,
    Json(payload): Json<UpsertProviderRequest>,
) -> impl IntoResponse {
    match state.db.upsert_provider_api_key(&provider, &payload.api_key).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_provider(
    State(state): State<AppState>,
    AxumPath(provider): AxumPath<String>,
) -> impl IntoResponse {
    match state.db.delete_provider(&provider).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "provider not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_policies(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    match state.db.list_policies(limit).await {
        Ok(policies) => (StatusCode::OK, Json(json!({ "policies": policies }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_policy(
    State(state): State<AppState>,
    AxumPath(policy_id): AxumPath<String>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_policy(&policy_id).await {
        Ok(Some(policy)) => (StatusCode::OK, Json(json!({ "policy": policy }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_policy(
    State(state): State<AppState>,
    AxumPath(policy_id): AxumPath<String>,
    Json(payload): Json<UpsertPolicyRequest>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.upsert_policy(&policy_id, &payload.policy).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_policy(
    State(state): State<AppState>,
    AxumPath(policy_id): AxumPath<String>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_policy(&policy_id).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_policy_uses(
    State(state): State<AppState>,
    AxumPath(policy_id): AxumPath<String>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_policy(&policy_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
        Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
    let limit = query.limit.unwrap_or(200);
    match state.db.list_policy_uses(&policy_id, limit).await {
        Ok(uses) => (StatusCode::OK, Json(json!({ "uses": uses }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_attach_policy_to_entity(
    State(state): State<AppState>,
    AxumPath((policy_id, entity_id)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let entity_id = match parse_uri_field("entity_id", &entity_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_policy(&policy_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
        Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
    match state.db.attach_policy_to_entity(&policy_id, &entity_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_detach_policy_from_entity(
    State(state): State<AppState>,
    AxumPath((policy_id, entity_id)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let policy_id = match parse_uri_field("policy_id", &policy_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let entity_id = match parse_uri_field("entity_id", &entity_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .detach_policy_from_entity(&policy_id, &entity_id)
        .await
    {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "policy association not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_agent_specs(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    match state.db.list_agent_specs(limit).await {
        Ok(specs) => (StatusCode::OK, Json(json!({ "agent_specs": specs }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_agent_spec(
    State(state): State<AppState>,
    AxumPath(agent_id): AxumPath<String>,
) -> impl IntoResponse {
    let agent_id = match parse_uri_field("agent_id", &agent_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_agent_spec(&agent_id).await {
        Ok(Some(spec)) => (StatusCode::OK, Json(json!({ "agent_spec": spec }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "agent spec not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_agent_spec(
    State(state): State<AppState>,
    AxumPath(agent_id): AxumPath<String>,
    Json(payload): Json<UpsertAgentSpecRequest>,
) -> impl IntoResponse {
    let agent_id = match parse_uri_field("agent_id", &agent_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .upsert_agent_spec(
            &agent_id,
            &payload.model,
            &payload.system_prompt,
            &payload.tools,
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_agent_spec(
    State(state): State<AppState>,
    AxumPath(agent_id): AxumPath<String>,
) -> impl IntoResponse {
    let agent_id = match parse_uri_field("agent_id", &agent_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_agent_spec(&agent_id).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "agent spec not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_users(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    match state.db.list_users(limit).await {
        Ok(users) => (StatusCode::OK, Json(json!({ "users": users }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_user(
    State(state): State<AppState>,
    Json(payload): Json<UpsertUserRequest>,
) -> impl IntoResponse {
    let user_key = match parse_uri_field("user_key", &payload.user_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.upsert_user(&user_key, &payload.profile).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_user(
    State(state): State<AppState>,
    AxumPath(user_key): AxumPath<String>,
) -> impl IntoResponse {
    let user_key = match parse_uri_field("user_key", &user_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_user(&user_key).await {
        Ok(Some(user)) => (StatusCode::OK, Json(json!({ "user": user }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "user not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_patch_user(
    State(state): State<AppState>,
    AxumPath(user_key): AxumPath<String>,
    Json(payload): Json<PatchUserRequest>,
) -> impl IntoResponse {
    let user_key = match parse_uri_field("user_key", &user_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.upsert_user(&user_key, &payload.profile).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_user(
    State(state): State<AppState>,
    AxumPath(user_key): AxumPath<String>,
) -> impl IntoResponse {
    let user_key = match parse_uri_field("user_key", &user_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_user(&user_key).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "user not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_sessions(
    State(state): State<AppState>,
    Query(query): Query<SessionsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    let user_key = match query.user_key {
        Some(raw) => match parse_uri_field("user_key", &raw) {
            Ok(v) => Some(v),
            Err(err) => return err,
        },
        None => None,
    };
    match state
        .db
        .list_sessions(limit, query.port.as_deref(), user_key.as_ref())
        .await
    {
        Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_session(
    State(state): State<AppState>,
    Json(payload): Json<UpsertSessionRequest>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &payload.session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_key = match parse_uri_field("user_key", &payload.user_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let root_task_id = match parse_uri_field("root_task_id", &payload.root_task_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .upsert_session(
            &session_id,
            &user_key,
            &payload.port,
            &root_task_id,
            &payload.state,
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_session(&session_id).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!({ "session": session }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "session not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_patch_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<PatchSessionRequest>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let Some(existing) = (match state.db.get_session(&session_id).await {
        Ok(v) => v,
        Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }) else {
        return api_error(StatusCode::NOT_FOUND, "session not found".to_string());
    };

    let user_key = match payload.user_key {
        Some(raw) => match parse_uri_field("user_key", &raw) {
            Ok(v) => v,
            Err(err) => return err,
        },
        None => existing.user_key,
    };
    let root_task_id = match payload.root_task_id {
        Some(raw) => match parse_uri_field("root_task_id", &raw) {
            Ok(v) => v,
            Err(err) => return err,
        },
        None => existing.root_task_id,
    };
    let port = payload.port.unwrap_or(existing.port);
    let state_value = payload.state.unwrap_or(existing.state);

    match state
        .db
        .upsert_session(&session_id, &user_key, &port, &root_task_id, &state_value)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_session(&session_id).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "session not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_append_session_message(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<UpsertSessionMessageRequest>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.append_session_message(&session_id, &payload.payload).await {
        Ok(message_index) => {
            (StatusCode::OK, Json(json!({ "message_index": message_index }))).into_response()
        }
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_session_messages(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<SessionMessagesQuery>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let from = query.from.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);
    match state.db.list_session_messages(&session_id, from, limit).await {
        Ok(messages) => (StatusCode::OK, Json(json!({ "messages": messages }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_session_message(
    State(state): State<AppState>,
    AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_session_message(&session_id, message_index).await {
        Ok(Some(message)) => (StatusCode::OK, Json(json!({ "message": message }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "session message not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_patch_session_message(
    State(state): State<AppState>,
    AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
    Json(payload): Json<UpsertSessionMessageRequest>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .update_session_message(&session_id, message_index, &payload.payload)
        .await
    {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "session message not found".to_string()),
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_session_message(
    State(state): State<AppState>,
    AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_session_message(&session_id, message_index).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "session message not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_clear_session_messages(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.clear_session_history(&session_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_port_settings(
    State(state): State<AppState>,
    AxumPath(port): AxumPath<String>,
    Query(query): Query<PortSettingsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(200);
    match state.db.list_port_settings(&port, limit).await {
        Ok(items) => {
            let settings: Vec<Value> = items
                .into_iter()
                .map(|(key, value)| json!({ "key": key, "value": value }))
                .collect();
            (StatusCode::OK, Json(json!({ "settings": settings }))).into_response()
        }
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_port_setting(
    State(state): State<AppState>,
    AxumPath((port, key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    match state.db.get_port_setting(&port, &key).await {
        Ok(Some(value)) => {
            (StatusCode::OK, Json(json!({ "port": port, "key": key, "value": value })))
                .into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_port_setting(
    State(state): State<AppState>,
    AxumPath((port, key)): AxumPath<(String, String)>,
    Json(payload): Json<UpsertPortSettingRequest>,
) -> impl IntoResponse {
    match state.db.upsert_port_setting(&port, &key, &payload.value).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_port_setting(
    State(state): State<AppState>,
    AxumPath((port, key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    match state.db.delete_port_setting(&port, &key).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_list_port_bindings(
    State(state): State<AppState>,
    AxumPath(port): AxumPath<String>,
    Query(query): Query<PortBindingsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(200);
    match state.db.list_port_bindings(&port, limit).await {
        Ok(items) => {
            let bindings: Vec<Value> = items
                .into_iter()
                .map(|(conversation_key, session_id, agent_id)| {
                    json!({
                        "conversation_key": conversation_key,
                        "session_id": session_id,
                        "agent_id": agent_id
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "bindings": bindings }))).into_response()
        }
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_port_binding(
    State(state): State<AppState>,
    AxumPath((port, conversation_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .get_port_binding_record(&port, &conversation_key)
        .await
    {
        Ok(Some((conversation_key, session_id, agent_id))) => (
            StatusCode::OK,
            Json(json!({
                "binding": {
                    "conversation_key": conversation_key,
                    "session_id": session_id,
                    "agent_id": agent_id
                }
            })),
        )
            .into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "port binding not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_port_binding(
    State(state): State<AppState>,
    AxumPath((port, conversation_key)): AxumPath<(String, String)>,
    Json(payload): Json<UpsertPortBindingRequest>,
) -> impl IntoResponse {
    let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let session_id = match parse_uri_field("session_id", &payload.session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let agent_id = match payload.agent_id {
        Some(raw) => match parse_uri_field("agent_id", &raw) {
            Ok(v) => Some(v),
            Err(err) => return err,
        },
        None => None,
    };

    match state
        .db
        .upsert_port_binding_record(&port, &conversation_key, &session_id, agent_id.as_ref())
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_port_binding(
    State(state): State<AppState>,
    AxumPath((port, conversation_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.delete_port_binding(&port, &conversation_key).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "port binding not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_port_session_context(
    State(state): State<AppState>,
    AxumPath((port, session_id)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_port_session_context(&port, &session_id).await {
        Ok(Some(ctx)) => {
            (StatusCode::OK, Json(json!({ "port": port, "session_id": session_id, "ctx": ctx })))
                .into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "port session context not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_upsert_port_session_context(
    State(state): State<AppState>,
    AxumPath((port, session_id)): AxumPath<(String, String)>,
    Json(payload): Json<UpsertPortSessionContextRequest>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state
        .db
        .upsert_port_session_context(&port, &session_id, &payload.ctx)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_delete_port_session_context(
    State(state): State<AppState>,
    AxumPath((port, session_id)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.clear_port_session_context(&port, &session_id).await {
        Ok(0) => api_error(StatusCode::NOT_FOUND, "port session context not found".to_string()),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn api_get_any_port_session_context(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> impl IntoResponse {
    let session_id = match parse_uri_field("session_id", &session_id) {
        Ok(v) => v,
        Err(err) => return err,
    };
    match state.db.get_any_port_session_context(&session_id).await {
        Ok(Some((port, ctx))) => {
            (StatusCode::OK, Json(json!({ "port": port, "session_id": session_id, "ctx": ctx })))
                .into_response()
        }
        Ok(None) => api_error(StatusCode::NOT_FOUND, "session context not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
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
