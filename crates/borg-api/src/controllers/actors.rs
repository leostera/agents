use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;

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
    #[serde(default)]
    status: Option<String>,
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

    pub(crate) async fn upsert_actor(
        State(state): State<AppState>,
        AxumPath(actor_id): AxumPath<String>,
        Json(payload): Json<UpsertActorRequest>,
    ) -> impl IntoResponse {
        let actor_id = match parse_uri_field("actor_id", &actor_id) {
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
            .upsert_actor(&actor_id, &name, &payload.system_prompt, status)
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
}

fn fallback_actor_name(actor_id: &str) -> String {
    actor_id
        .rsplit(':')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("actor")
        .to_string()
}
