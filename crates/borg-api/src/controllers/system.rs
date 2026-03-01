use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use borg_core::Uri;
use borg_exec::UserMessage;
use borg_ports::{BORG_SESSION_ID_HEADER, Port, PortMessage};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, info};

use crate::AppState;
use crate::controllers::common::{ApiFieldError, ApiValidationError, api_error};

const HEALTH_STATUS_OK: &str = "ok";

#[derive(Deserialize)]
pub(crate) struct MemorySearchQuery {
    q: String,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct MemoryExplorerQuery {
    query: String,
    limit: Option<usize>,
    max_nodes: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct HttpPortRequest {
    pub(crate) user_key: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default)]
    pub(crate) metadata: Option<Value>,
}

pub(crate) struct SystemController;

impl SystemController {
    pub(crate) async fn health() -> impl IntoResponse {
        debug!(target: "borg_api", "health endpoint called");
        Json(json!({ "status": HEALTH_STATUS_OK }))
    }

    pub(crate) async fn ui_dashboard(State(state): State<AppState>) -> impl IntoResponse {
        debug!(target: "borg_api", "ui dashboard endpoint called");
        let entities_count = state
            .memory
            .search("movie", None, 10_000)
            .await
            .map(|v| v.len())
            .unwrap_or(0);
        Html(Self::render_dashboard(entities_count))
    }

    pub(crate) async fn ports_http(
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
                        "session_id": message.session_id,
                        "reply": message.reply
                    })),
                )
                    .into_response();
                if let Some(session_id) = message.session_id
                    && let Ok(value) = session_id.to_string().parse()
                {
                    response.headers_mut().insert(BORG_SESSION_ID_HEADER, value);
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

    pub(crate) async fn memory_search(
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

    pub(crate) async fn get_memory_entity(
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

    pub(crate) async fn memory_explorer(
        State(state): State<AppState>,
        Query(query): Query<MemoryExplorerQuery>,
    ) -> impl IntoResponse {
        let search_query = query.query.trim().to_string();
        if search_query.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, "query is required".to_string());
        }

        let limit = query.limit.unwrap_or(25);
        let max_nodes = query.max_nodes.unwrap_or(300);
        debug!(
            target: "borg_api",
            query = search_query,
            limit,
            max_nodes,
            "memory explorer endpoint"
        );

        match state.memory.explore(&search_query, limit, max_nodes).await {
            Ok(result) => (
                StatusCode::OK,
                Json(json!({ "entities": result.entities, "edges": result.edges })),
            )
                .into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    fn render_dashboard(entities_count: usize) -> String {
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Borg Dashboard</title></head><body><h1>Borg Dashboard</h1><ul><li>Memory entities: {entities_count}</li></ul></body></html>"
        )
    }
}

pub(crate) fn validate_port_request(
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
