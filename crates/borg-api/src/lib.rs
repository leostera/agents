use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use borg_db::BorgDb;
use borg_exec::{ExecEngine, InboxMessage};
use borg_ltm::MemoryStore;
use borg_ports::{BORG_SESSION_ID_HEADER, HttpPort};
use borg_ui::render_dashboard;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{debug, info};

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
}

impl BorgApiServer {
    pub fn new(bind: String, db: BorgDb, exec: ExecEngine, memory: MemoryStore) -> Self {
        Self {
            bind,
            state: AppState {
                db,
                http_port: HttpPort::new(exec),
                memory,
            },
        }
    }

    pub async fn run(self) -> Result<()> {
        let router = Router::new()
            .route("/", get(ui_dashboard))
            .route("/health", get(health))
            .route("/ports/http", post(ports_http))
            .route("/tasks", get(list_tasks))
            .route("/tasks/:id", get(get_task))
            .route("/tasks/:id/events", get(get_task_events))
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

        Ok(())
    }
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
    Json(payload): Json<InboxMessage>,
) -> impl IntoResponse {
    info!(target: "borg_api", user_key = payload.user_key, text = payload.text, "received HTTP port event");
    match state.http_port.inbox(&headers, payload).await {
        Ok((port_response, session_header)) => {
            let mut response = (StatusCode::OK, Json(json!(port_response))).into_response();
            if let Some(value) = session_header {
                response.headers_mut().insert(BORG_SESSION_ID_HEADER, value);
            }
            response
        }
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
    match state.db.get_task_events(&task_id).await {
        Ok(events) => (StatusCode::OK, Json(json!({ "events": events }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
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
