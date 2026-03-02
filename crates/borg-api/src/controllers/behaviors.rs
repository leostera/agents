use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
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
pub(crate) struct UpsertBehaviorRequest {
    #[serde(default)]
    name: Option<String>,
    system_prompt: String,
    #[serde(default)]
    preferred_provider_id: Option<String>,
    #[serde(default)]
    required_capabilities_json: Option<Value>,
    #[serde(default)]
    session_turn_concurrency: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

pub(crate) struct BehaviorsController;

impl BehaviorsController {
    pub(crate) async fn list_behaviors(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_behaviors(limit).await {
            Ok(behaviors) => {
                (StatusCode::OK, Json(json!({ "behaviors": behaviors }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_behavior(
        State(state): State<AppState>,
        AxumPath(behavior_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let behavior_id = match parse_uri_field("behavior_id", &behavior_id) {
            Ok(value) => value,
            Err(err) => return err,
        };
        match state.db.get_behavior(&behavior_id).await {
            Ok(Some(behavior)) => {
                (StatusCode::OK, Json(json!({ "behavior": behavior }))).into_response()
            }
            Ok(None) => api_error(StatusCode::NOT_FOUND, "behavior not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_behavior(
        State(state): State<AppState>,
        AxumPath(behavior_id): AxumPath<String>,
        Json(payload): Json<UpsertBehaviorRequest>,
    ) -> impl IntoResponse {
        let behavior_id = match parse_uri_field("behavior_id", &behavior_id) {
            Ok(value) => value,
            Err(err) => return err,
        };

        let name = payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| fallback_behavior_name(&behavior_id.to_string()));
        let preferred_provider_id = payload
            .preferred_provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let required_capabilities_json = payload.required_capabilities_json.unwrap_or(json!([]));
        let session_turn_concurrency = payload
            .session_turn_concurrency
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("serial");
        let status = payload
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("ACTIVE");

        match state
            .db
            .upsert_behavior(
                &behavior_id,
                &name,
                &payload.system_prompt,
                preferred_provider_id.as_deref(),
                &required_capabilities_json,
                session_turn_concurrency,
                status,
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_behavior(
        State(state): State<AppState>,
        AxumPath(behavior_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let behavior_id = match parse_uri_field("behavior_id", &behavior_id) {
            Ok(value) => value,
            Err(err) => return err,
        };
        match state.db.delete_behavior(&behavior_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "behavior not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}

fn fallback_behavior_name(behavior_id: &str) -> String {
    behavior_id
        .rsplit(':')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("behavior")
        .to_string()
}
