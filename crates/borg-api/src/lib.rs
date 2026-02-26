use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
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
        let router = Router::new()
            .route("/", get(ui_dashboard))
            .route("/health", get(health))
            .route("/ports/http", post(ports_http))
            .route("/tasks", get(list_tasks))
            .route("/tasks/:id", get(get_task))
            .route("/tasks/:id/events", get(get_task_events))
            .route("/tasks/:id/output", get(get_task_output))
            .route("/memory/search", get(memory_search))
            .route("/memory/entities/:id", get(get_memory_entity))
            .with_state(self.state);

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
    use super::{HttpPortRequest, validate_port_request};
    use serde_json::json;

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
}
