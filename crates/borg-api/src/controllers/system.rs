use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use borg_core::{Entity, Uri};
use borg_exec::UserMessage;
use borg_memory::{FactArity, FactValue, Uri as MemoryUri};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::debug;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct HttpPortRequest {
    pub user_key: String,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
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
            Ok(entities) => {
                let mut serialized = Vec::with_capacity(entities.len());
                for entity in entities {
                    match hydrate_entity_props_with_field_uris(&state, entity).await {
                        Ok(normalized) => serialized.push(normalized),
                        Err(err) => {
                            return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                        }
                    }
                }
                (StatusCode::OK, Json(json!({ "entities": serialized }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_memory_entity(
        State(state): State<AppState>,
        AxumPath(entity_id): AxumPath<String>,
    ) -> impl IntoResponse {
        debug!(target: "borg_api", entity_id, "get memory entity endpoint");
        match state.memory.get_entity(&entity_id).await {
            Ok(Some(entity)) => match hydrate_entity_props_with_field_uris(&state, entity).await {
                Ok(normalized) => {
                    (StatusCode::OK, Json(json!({ "entity": normalized }))).into_response()
                }
                Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
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
            Ok(result) => {
                let mut serialized = Vec::with_capacity(result.entities.len());
                for entity in result.entities {
                    match hydrate_entity_props_with_field_uris(&state, entity).await {
                        Ok(normalized) => serialized.push(normalized),
                        Err(err) => {
                            return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                        }
                    }
                }
                (
                    StatusCode::OK,
                    Json(json!({ "entities": serialized, "edges": result.edges })),
                )
                    .into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn ports_http(
        State(_state): State<AppState>,
        _headers: HeaderMap,
        Json(payload): Json<HttpPortRequest>,
    ) -> impl IntoResponse {
        match validate_port_request(payload) {
            Ok(_validated) => api_error(
                StatusCode::NOT_IMPLEMENTED,
                "ports/http is temporarily unavailable during refactor".to_string(),
            ),
            Err(err) => err,
        }
    }

    fn render_dashboard(entities_count: usize) -> String {
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Borg Dashboard</title></head><body><h1>Borg Dashboard</h1><ul><li>Memory entities: {entities_count}</li></ul></body></html>"
        )
    }
}

async fn hydrate_entity_props_with_field_uris(
    state: &AppState,
    mut entity: Entity,
) -> anyhow::Result<Entity> {
    let entity_uri = MemoryUri::parse(entity.entity_id.to_string())?;
    let facts = state
        .memory
        .list_facts(Some(&entity_uri), None, false, 5000)
        .await?;
    let mut props = serde_json::Map::new();
    for fact in facts {
        let key = fact.field.to_string();
        let value = fact_value_to_json(&fact.value);
        match fact.arity {
            FactArity::One => {
                props.insert(key, value);
            }
            FactArity::Many => match props.get_mut(&key) {
                Some(existing) => {
                    if let Value::Array(values) = existing {
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    } else if *existing != value {
                        let prior = existing.clone();
                        *existing = Value::Array(vec![prior, value]);
                    }
                }
                None => {
                    props.insert(key, Value::Array(vec![value]));
                }
            },
        }
    }
    entity.props = Value::Object(props);
    Ok(entity)
}

fn fact_value_to_json(value: &FactValue) -> Value {
    match value {
        FactValue::Text(v) => Value::String(v.clone()),
        FactValue::Integer(v) => Value::Number((*v).into()),
        FactValue::Float(v) => serde_json::Number::from_f64(*v)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FactValue::Boolean(v) => Value::Bool(*v),
        FactValue::Bytes(v) => Value::Array(v.iter().map(|b| Value::Number((*b).into())).collect()),
        FactValue::Ref(v) => Value::String(v.to_string()),
        FactValue::Json(v) => v.clone(),
    }
}

pub(crate) fn validate_port_request(
    payload: HttpPortRequest,
) -> Result<UserMessage, axum::response::Response> {
    let mut details = Vec::new();
    let user_id = match Uri::parse(&payload.user_key) {
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
        user_id: user_id.expect("validated user_key"),
        text: payload.text,
        session_id,
        agent_id,
        metadata: payload
            .metadata
            .unwrap_or(Value::Object(Default::default())),
    })
}
