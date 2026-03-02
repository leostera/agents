use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::controllers::common::{api_error, parse_uri_field};

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPortActorBindingRequest {
    actor_id: String,
}

pub(crate) struct PortActorBindingsController;

impl PortActorBindingsController {
    pub(crate) async fn list_port_actor_bindings(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let port_name = match port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let limit = query.limit.unwrap_or(200);

        match state.db.list_port_actor_bindings(&port_name, limit).await {
            Ok(rows) => {
                let bindings: Vec<Value> = rows
                    .into_iter()
                    .map(|(conversation_key, actor_id)| {
                        json!({
                            "conversation_key": conversation_key,
                            "actor_id": actor_id,
                        })
                    })
                    .collect();
                (StatusCode::OK, Json(json!({ "bindings": bindings }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_actor_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(value) => value,
            Err(err) => return err,
        };

        match state
            .db
            .get_port_actor_binding(&port_name, &conversation_key)
            .await
        {
            Ok(Some(actor_id)) => (
                StatusCode::OK,
                Json(json!({
                    "binding": {
                        "conversation_key": conversation_key,
                        "actor_id": actor_id,
                    }
                })),
            )
                .into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "actor binding not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port_actor_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortActorBindingRequest>,
    ) -> impl IntoResponse {
        let port_name = match port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(value) => value,
            Err(err) => return err,
        };
        let actor_id = match parse_uri_field("actor_id", &payload.actor_id) {
            Ok(value) => value,
            Err(err) => return err,
        };

        match state
            .db
            .upsert_port_actor_binding(&port_name, &conversation_key, &actor_id)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_actor_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(value) => value,
            Err(err) => return err,
        };

        match state
            .db
            .clear_port_actor_binding(&port_name, &conversation_key)
            .await
        {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "actor binding not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}

async fn port_name_from_uri(state: &AppState, port_uri: &str) -> Result<String, Response> {
    let port_id = parse_uri_field("port_uri", port_uri)?;
    let raw = port_id.to_string();
    let mut parts = raw.splitn(3, ':');
    let ns = parts.next().unwrap_or_default();
    let kind = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    if ns != "borg" || kind != "port" || id.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "port_uri must be in the format borg:port:<name>".to_string(),
        ));
    }

    match state.db.get_port_by_id(&port_id).await {
        Ok(Some(port)) => Ok(port.port_name),
        Ok(None) => Ok(id.to_string()),
        Err(err) => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{err:#}"),
        )),
    }
}
