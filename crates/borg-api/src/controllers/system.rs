use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
};
use base64::Engine;
use borg_core::{Entity, EntityPropValue, Uri, uri};
use borg_exec::{BorgCommand, BorgInput, BorgMessage, HttpSessionContext, PortContext};
use borg_fs::{FileKind, PutFileMetadata};
use borg_memory::{FactArity, FactValue, Uri as MemoryUri};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::debug;

use crate::AppState;
use crate::controllers::common::{ApiFieldError, ApiValidationError, api_error};

const HEALTH_STATUS_OK: &str = "ok";
const HTTP_PORT_NAME: &str = "http";
const HTTP_PORT_URI_NAMESPACE: &str = "borg";
const HTTP_PORT_URI_KIND: &str = "port";
const HTTP_HELP_TEXT: &str = "Available commands: /help, /start, /model [model_name], /participants, /context, /reset, /compact";
const HTTP_START_GREETING: &str = "Borg is online. Send a message to start.";
const MODEL_COMMAND_USAGE: &str = "Usage: /model [model_name]";
const BORG_SESSION_ID_HEADER: &str = "x-borg-session-id";

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct HttpPortAudioRequest {
    pub user_key: String,
    pub audio_base64: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub language_hint: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

struct ValidatedHttpPortAudioRequest {
    user_id: Uri,
    audio_bytes: Vec<u8>,
    mime_type: Option<String>,
    duration_ms: Option<u64>,
    language_hint: Option<String>,
    session_id: Option<Uri>,
    agent_id: Option<Uri>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedPortRequest {
    pub user_id: Uri,
    pub text: String,
    pub session_id: Option<Uri>,
    pub agent_id: Option<Uri>,
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
        State(state): State<AppState>,
        headers: HeaderMap,
        Json(payload): Json<HttpPortRequest>,
    ) -> impl IntoResponse {
        match validate_port_request(payload) {
            Ok(validated) => {
                let conversation_key = validated
                    .session_id
                    .clone()
                    .unwrap_or_else(|| validated.user_id.clone());
                let requested_actor_id = validated
                    .agent_id
                    .as_ref()
                    .filter(|value| value.as_str().contains(":actor:"));
                let (session_id, legacy_actor_id) = match state
                    .db
                    .resolve_port_session(
                        HTTP_PORT_NAME,
                        &conversation_key,
                        validated.session_id.as_ref(),
                        validated.agent_id.as_ref(),
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let default_actor_id = match state.db.get_port(HTTP_PORT_NAME).await {
                    Ok(Some(port)) => port.assigned_actor_id,
                    Ok(None) => None,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let resolved_actor_id = match state
                    .db
                    .resolve_port_actor(
                        HTTP_PORT_NAME,
                        &conversation_key,
                        requested_actor_id,
                        default_actor_id.as_ref(),
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                if let Err(err) = ensure_session_row(
                    &state,
                    &headers,
                    &validated.user_id,
                    &session_id,
                    HTTP_PORT_NAME,
                )
                .await
                {
                    return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                }

                let input = match resolve_http_port_input(&validated.text) {
                    Ok(value) => value,
                    Err(err) => return api_error(StatusCode::BAD_REQUEST, err),
                };

                let body = match input {
                    HttpPortInput::LocalReply(reply) => json!({
                        "session_id": session_id,
                        "reply": reply,
                        "tool_calls": [],
                    }),
                    HttpPortInput::Forward(forward_input) => match state
                        .supervisor
                        .call(BorgMessage {
                            actor_id: select_actor_id(
                                session_id.clone(),
                                Some(resolved_actor_id),
                                legacy_actor_id,
                            ),
                            user_id: validated.user_id,
                            session_id: session_id.clone(),
                            input: forward_input,
                            port_context: PortContext::Http(HttpSessionContext::default()),
                        })
                        .await
                    {
                        Ok(output) => json!({
                            "session_id": output.session_id,
                            "reply": output.reply,
                            "tool_calls": output
                                .tool_calls
                                .into_iter()
                                .map(|call| {
                                    json!({
                                        "tool_name": call.tool_name,
                                        "arguments": call.arguments,
                                        "output": call.output,
                                    })
                                })
                                .collect::<Vec<_>>(),
                        }),
                        Err(err) => {
                            return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                        }
                    },
                };

                let mut response = (StatusCode::OK, Json(body)).into_response();
                if let Ok(value) = HeaderValue::from_str(session_id.as_str()) {
                    response.headers_mut().insert(BORG_SESSION_ID_HEADER, value);
                }
                response
            }
            Err(err) => err,
        }
    }

    pub(crate) async fn ports_http_audio(
        State(state): State<AppState>,
        headers: HeaderMap,
        Json(payload): Json<HttpPortAudioRequest>,
    ) -> impl IntoResponse {
        match validate_audio_port_request(payload) {
            Ok(validated) => {
                let conversation_key = validated
                    .session_id
                    .clone()
                    .unwrap_or_else(|| validated.user_id.clone());
                let requested_actor_id = validated
                    .agent_id
                    .as_ref()
                    .filter(|value| value.as_str().contains(":actor:"));
                let (session_id, legacy_actor_id) = match state
                    .db
                    .resolve_port_session(
                        HTTP_PORT_NAME,
                        &conversation_key,
                        validated.session_id.as_ref(),
                        validated.agent_id.as_ref(),
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let default_actor_id = match state.db.get_port(HTTP_PORT_NAME).await {
                    Ok(Some(port)) => port.assigned_actor_id,
                    Ok(None) => None,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let resolved_actor_id = match state
                    .db
                    .resolve_port_actor(
                        HTTP_PORT_NAME,
                        &conversation_key,
                        requested_actor_id,
                        default_actor_id.as_ref(),
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                if let Err(err) = ensure_session_row(
                    &state,
                    &headers,
                    &validated.user_id,
                    &session_id,
                    HTTP_PORT_NAME,
                )
                .await
                {
                    return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                }

                let file = match state
                    .files
                    .put_bytes(
                        FileKind::Audio,
                        &validated.audio_bytes,
                        PutFileMetadata {
                            session_id: session_id.clone(),
                        },
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let output = match state
                    .supervisor
                    .call(BorgMessage {
                        actor_id: select_actor_id(
                            session_id.clone(),
                            Some(resolved_actor_id),
                            legacy_actor_id,
                        ),
                        user_id: validated.user_id,
                        session_id: session_id.clone(),
                        input: BorgInput::Audio {
                            file_id: file.file_id,
                            mime_type: validated.mime_type,
                            duration_ms: validated.duration_ms,
                            language_hint: validated.language_hint,
                        },
                        port_context: PortContext::Http(HttpSessionContext::default()),
                    })
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                    }
                };

                let mut response = (
                    StatusCode::OK,
                    Json(json!({
                        "session_id": output.session_id,
                        "reply": output.reply,
                        "tool_calls": output
                            .tool_calls
                            .into_iter()
                            .map(|call| {
                                json!({
                                    "tool_name": call.tool_name,
                                    "arguments": call.arguments,
                                    "output": call.output,
                                })
                            })
                            .collect::<Vec<_>>(),
                    })),
                )
                    .into_response();
                if let Ok(value) = HeaderValue::from_str(session_id.as_str()) {
                    response.headers_mut().insert(BORG_SESSION_ID_HEADER, value);
                }
                response
            }
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
    let mut props = borg_core::EntityProps::new();
    for fact in facts {
        let key = fact.field.to_string();
        let value = fact_value_to_prop(&fact.value)?;
        match fact.arity {
            FactArity::One => {
                props.insert(key, value);
            }
            FactArity::Many => match props.get_mut(&key) {
                Some(existing) => {
                    if let EntityPropValue::List(values) = existing {
                        if !values.contains(&value) {
                            values.push(value);
                        }
                    } else if *existing != value {
                        let prior = existing.clone();
                        *existing = EntityPropValue::List(vec![prior, value]);
                    }
                }
                None => {
                    props.insert(key, EntityPropValue::List(vec![value]));
                }
            },
        }
    }
    entity.props = props;
    Ok(entity)
}

fn fact_value_to_prop(value: &FactValue) -> anyhow::Result<EntityPropValue> {
    Ok(match value {
        FactValue::Text(v) => EntityPropValue::Text(v.clone()),
        FactValue::Integer(v) => EntityPropValue::Integer(*v),
        FactValue::Float(v) => EntityPropValue::Float(*v),
        FactValue::Boolean(v) => EntityPropValue::Boolean(*v),
        FactValue::Bytes(v) => EntityPropValue::Bytes(v.clone()),
        FactValue::Ref(v) => EntityPropValue::Ref(Uri::parse(v.as_str())?),
        FactValue::Json(v) => EntityPropValue::Text(serde_json::to_string(v)?),
    })
}

pub(crate) fn validate_port_request(
    payload: HttpPortRequest,
) -> Result<ValidatedPortRequest, axum::response::Response> {
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

    if payload.text.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "text is required".to_string(),
        ));
    }

    Ok(ValidatedPortRequest {
        user_id: user_id.expect("validated user_key"),
        text: payload.text,
        session_id,
        agent_id,
    })
}

fn validate_audio_port_request(
    payload: HttpPortAudioRequest,
) -> Result<ValidatedHttpPortAudioRequest, axum::response::Response> {
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

    let audio_base64 = payload.audio_base64.trim();
    if audio_base64.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "audio_base64 is required".to_string(),
        ));
    }

    let audio_bytes = match base64::engine::general_purpose::STANDARD.decode(audio_base64) {
        Ok(value) if !value.is_empty() => value,
        Ok(_) => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "audio_base64 decoded to empty payload".to_string(),
            ));
        }
        Err(err) => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("audio_base64 is invalid: {}", err),
            ));
        }
    };

    Ok(ValidatedHttpPortAudioRequest {
        user_id: user_id.expect("validated user_key"),
        audio_bytes,
        mime_type: payload
            .mime_type
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        duration_ms: payload.duration_ms,
        language_hint: payload
            .language_hint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        session_id,
        agent_id,
    })
}

#[derive(Debug, Clone)]
enum HttpPortInput {
    LocalReply(String),
    Forward(BorgInput),
}

fn resolve_http_port_input(text: &str) -> Result<HttpPortInput, String> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return Ok(HttpPortInput::Forward(BorgInput::Chat {
            text: text.to_string(),
        }));
    }

    let mut parts = trimmed.split_whitespace();
    let token = parts.next().unwrap_or_default();
    let command = token
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let args: Vec<String> = parts.map(ToOwned::to_owned).collect();

    match command.as_str() {
        "help" => Ok(HttpPortInput::LocalReply(HTTP_HELP_TEXT.to_string())),
        "start" => Ok(HttpPortInput::LocalReply(HTTP_START_GREETING.to_string())),
        "model" => parse_model_command_action(&args),
        "participants" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ParticipantsList,
        ))),
        "context" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ContextDump,
        ))),
        "reset" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ResetContext,
        ))),
        "compact" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::CompactSession,
        ))),
        "" => Err("empty command".to_string()),
        _ => Err(format!("unknown command: /{command}")),
    }
}

fn parse_model_command_action(args: &[String]) -> Result<HttpPortInput, String> {
    match args {
        [] => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ModelShowCurrent,
        ))),
        [model] if !model.trim().is_empty() => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ModelSet {
                model: model.trim().to_string(),
            },
        ))),
        [..] => Err(MODEL_COMMAND_USAGE.to_string()),
    }
}

async fn ensure_session_row(
    state: &AppState,
    headers: &HeaderMap,
    user_id: &Uri,
    session_id: &Uri,
    port_name: &str,
) -> anyhow::Result<()> {
    let mut users = state
        .db
        .get_session(session_id)
        .await?
        .map(|session| session.users)
        .unwrap_or_default();
    if !users.iter().any(|value| value == user_id) {
        users.push(user_id.clone());
    }
    if users.is_empty() {
        users.push(user_id.clone());
    }

    let requested_port_id = headers
        .get("x-borg-port-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uri::parse(value).ok());
    let port_id = requested_port_id
        .unwrap_or_else(|| uri!(HTTP_PORT_URI_NAMESPACE, HTTP_PORT_URI_KIND, port_name));
    state.db.upsert_session(session_id, &users, &port_id).await
}

fn select_actor_id(
    session_id: Uri,
    bound_actor_id: Option<Uri>,
    legacy_actor_id: Option<Uri>,
) -> Uri {
    if let Some(actor_id) = bound_actor_id {
        return actor_id;
    }
    if let Some(actor_id) = legacy_actor_id.filter(|value| value.as_str().contains(":actor:")) {
        return actor_id;
    }
    session_id
}

#[cfg(test)]
mod tests {
    use super::{
        HttpPortAudioRequest, HttpPortInput, MODEL_COMMAND_USAGE, resolve_http_port_input,
        validate_audio_port_request, validate_port_request,
    };
    use crate::controllers::system::HttpPortRequest;
    use borg_exec::{BorgCommand, BorgInput};
    use serde_json::json;

    #[test]
    fn validate_port_request_rejects_empty_text() {
        let request = HttpPortRequest {
            user_key: "borg:user:test".to_string(),
            text: "   ".to_string(),
            session_id: None,
            agent_id: None,
            metadata: Some(json!({})),
        };
        assert!(validate_port_request(request).is_err());
    }

    #[test]
    fn validate_audio_port_request_rejects_invalid_base64() {
        let request = HttpPortAudioRequest {
            user_key: "borg:user:test".to_string(),
            audio_base64: "not-base64".to_string(),
            mime_type: Some("audio/wav".to_string()),
            duration_ms: None,
            language_hint: None,
            session_id: None,
            agent_id: None,
            metadata: Some(json!({})),
        };
        assert!(validate_audio_port_request(request).is_err());
    }

    #[test]
    fn resolve_http_port_input_maps_model_show() {
        let input = resolve_http_port_input("/model").expect("parse /model");
        assert!(matches!(
            input,
            HttpPortInput::Forward(BorgInput::Command(BorgCommand::ModelShowCurrent))
        ));
    }

    #[test]
    fn resolve_http_port_input_maps_model_set() {
        let input = resolve_http_port_input("/model moonshotai/kimi-k2").expect("parse /model set");
        assert!(matches!(
            input,
            HttpPortInput::Forward(BorgInput::Command(BorgCommand::ModelSet { model }))
                if model == "moonshotai/kimi-k2"
        ));
    }

    #[test]
    fn resolve_http_port_input_reports_model_usage_for_invalid_shape() {
        let error =
            resolve_http_port_input("/model one two").expect_err("invalid /model should fail");
        assert_eq!(error, MODEL_COMMAND_USAGE);
    }
}
