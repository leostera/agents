use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use borg_exec::{BorgInput, BorgMessage, JsonPortContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;
use crate::controllers::common::{api_error, parse_uri_field};

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertActorRequest {
    #[serde(default)]
    name: Option<String>,
    system_prompt: String,
    default_behavior_id: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ActorChatRequest {
    session_id: String,
    user_id: String,
    text: String,
    #[serde(default)]
    metadata: Option<Value>,
}

pub(crate) struct ActorsController;

impl ActorsController {
    pub(crate) async fn list_actors(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_actors(limit).await {
            Ok(actors) => (StatusCode::OK, Json(json!({ "actors": actors }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_actor(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_actor(&actor_id).await {
            Ok(Some(actor)) => (StatusCode::OK, Json(json!({ "actor": actor }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "actor not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_actor_sessions(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let limit = query.limit.unwrap_or(100);
        match state.db.list_actor_sessions(&actor_id, limit).await {
            Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_actor(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
        Json(payload): Json<UpsertActorRequest>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let default_behavior_id =
            match parse_uri_field("default_behavior_id", &payload.default_behavior_id) {
                Ok(v) => v,
                Err(err) => return err,
            };
        let name = payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| fallback_actor_name(&actor_id.to_string()));
        let status = payload
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("RUNNING");
        match state
            .db
            .upsert_actor(
                &actor_id,
                &name,
                &payload.system_prompt,
                &default_behavior_id,
                status,
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_actor(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_actor(&actor_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "actor not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn chat(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
        Json(payload): Json<ActorChatRequest>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let session_id = match parse_uri_field("session_id", &payload.session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let user_id = match parse_uri_field("user_id", &payload.user_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let text = payload.text.trim();
        if text.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, "text is required".to_string());
        }

        let message = BorgMessage {
            actor_id,
            user_id,
            session_id,
            input: BorgInput::Chat {
                text: text.to_string(),
            },
            port_context: Arc::new(JsonPortContext::new(payload.metadata.unwrap_or_else(
                || {
                    json!({
                        "port": "devmode"
                    })
                },
            ))),
        };

        match state.supervisor.call(message).await {
            Ok(output) => (
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
                .into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}

fn fallback_actor_name(actor_id: &str) -> String {
    actor_id
        .rsplit(':')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("actor")
        .to_string()
}
