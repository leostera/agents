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

fn default_app_status() -> String {
    "active".to_string()
}

fn default_app_source() -> String {
    "custom".to_string()
}

fn default_app_auth_strategy() -> String {
    "none".to_string()
}

fn default_connection_status() -> String {
    "connected".to_string()
}

fn default_secret_kind() -> String {
    "opaque".to_string()
}

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAppRequest {
    name: String,
    slug: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_app_status")]
    status: String,
    #[serde(default = "default_app_source")]
    source: String,
    #[serde(default = "default_app_auth_strategy")]
    auth_strategy: String,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAppCapabilityRequest {
    name: String,
    #[serde(default)]
    hint: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    instructions: String,
    #[serde(default = "default_app_status")]
    status: String,
}

#[derive(Deserialize)]
pub(crate) struct AppSecretsQuery {
    limit: Option<usize>,
    connection_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAppConnectionRequest {
    #[serde(default)]
    owner_user_id: Option<String>,
    #[serde(default)]
    provider_account_id: Option<String>,
    #[serde(default)]
    external_user_id: Option<String>,
    #[serde(default = "default_connection_status")]
    status: String,
    #[serde(default)]
    connection_json: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAppSecretRequest {
    key: String,
    value: String,
    #[serde(default)]
    connection_id: Option<String>,
    #[serde(default = "default_secret_kind")]
    kind: String,
}

pub(crate) struct AppsController;

impl AppsController {
    pub(crate) async fn list_apps(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_apps(limit).await {
            Ok(apps) => (StatusCode::OK, Json(json!({ "apps": apps }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_app(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app(&app_id).await {
            Ok(Some(app)) => (StatusCode::OK, Json(json!({ "app": app }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "app not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn upsert_app(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
        Json(payload): Json<UpsertAppRequest>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };

        if payload.name.trim().is_empty() {
            return api_error(
                StatusCode::BAD_REQUEST,
                "name must not be empty".to_string(),
            );
        }
        if payload.slug.trim().is_empty() {
            return api_error(
                StatusCode::BAD_REQUEST,
                "slug must not be empty".to_string(),
            );
        }

        match state
            .db
            .upsert_app_with_metadata(
                &app_id,
                payload.name.trim(),
                payload.slug.trim(),
                payload.description.trim(),
                payload.status.trim(),
                false,
                payload.source.trim(),
                payload.auth_strategy.trim(),
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_app(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app(&app_id).await {
            Ok(Some(app)) if app.built_in => {
                return api_error(
                    StatusCode::FORBIDDEN,
                    "built-in apps cannot be deleted".to_string(),
                );
            }
            Ok(Some(_)) => {}
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "app not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        match state.db.delete_app(&app_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "app not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_app_capabilities(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app(&app_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "app not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        let limit = query.limit.unwrap_or(100);
        match state.db.list_app_capabilities(&app_id, limit).await {
            Ok(capabilities) => (
                StatusCode::OK,
                Json(json!({ "capabilities": capabilities })),
            )
                .into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_app_capability(
        State(state): State<AppState>,
        AxumPath((app_id, capability_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let capability_id = match parse_uri_field("capability_id", &capability_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app_capability(&app_id, &capability_id).await {
            Ok(Some(capability)) => {
                (StatusCode::OK, Json(json!({ "capability": capability }))).into_response()
            }
            Ok(None) => api_error(StatusCode::NOT_FOUND, "capability not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn upsert_app_capability(
        State(state): State<AppState>,
        AxumPath((app_id, capability_id)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertAppCapabilityRequest>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let capability_id = match parse_uri_field("capability_id", &capability_id) {
            Ok(v) => v,
            Err(err) => return err,
        };

        if payload.name.trim().is_empty() {
            return api_error(
                StatusCode::BAD_REQUEST,
                "name must not be empty".to_string(),
            );
        }
        match state.db.get_app(&app_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "app not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }

        let mode = if payload.mode.trim().is_empty() {
            "codemode"
        } else {
            payload.mode.trim()
        };
        let status = if payload.status.trim().is_empty() {
            "active"
        } else {
            payload.status.trim()
        };

        match state
            .db
            .upsert_app_capability(
                &app_id,
                &capability_id,
                payload.name.trim(),
                payload.hint.trim(),
                mode,
                payload.instructions.trim(),
                status,
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_app_capability(
        State(state): State<AppState>,
        AxumPath((app_id, capability_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let capability_id = match parse_uri_field("capability_id", &capability_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .delete_app_capability(&app_id, &capability_id)
            .await
        {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "capability not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_app_connections(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let limit = query.limit.unwrap_or(100);
        match state.db.list_app_connections(&app_id, limit).await {
            Ok(connections) => {
                (StatusCode::OK, Json(json!({ "connections": connections }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_app_connection(
        State(state): State<AppState>,
        AxumPath((app_id, connection_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let connection_id = match parse_uri_field("connection_id", &connection_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app_connection(&app_id, &connection_id).await {
            Ok(Some(connection)) => {
                (StatusCode::OK, Json(json!({ "connection": connection }))).into_response()
            }
            Ok(None) => api_error(StatusCode::NOT_FOUND, "connection not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn upsert_app_connection(
        State(state): State<AppState>,
        AxumPath((app_id, connection_id)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertAppConnectionRequest>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let connection_id = match parse_uri_field("connection_id", &connection_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let owner_user_id = match payload.owner_user_id {
            Some(raw) if !raw.trim().is_empty() => match parse_uri_field("owner_user_id", &raw) {
                Ok(value) => Some(value),
                Err(err) => return err,
            },
            _ => None,
        };
        match state
            .db
            .upsert_app_connection(
                &app_id,
                &connection_id,
                owner_user_id.as_ref(),
                payload.provider_account_id.as_deref(),
                payload.external_user_id.as_deref(),
                payload.status.trim(),
                &payload.connection_json,
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_app_connection(
        State(state): State<AppState>,
        AxumPath((app_id, connection_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let connection_id = match parse_uri_field("connection_id", &connection_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .delete_app_connection(&app_id, &connection_id)
            .await
        {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "connection not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_app_secrets(
        State(state): State<AppState>,
        AxumPath(app_id): AxumPath<String>,
        Query(query): Query<AppSecretsQuery>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let connection_id = match query.connection_id {
            Some(raw) if !raw.trim().is_empty() => match parse_uri_field("connection_id", &raw) {
                Ok(value) => Some(value),
                Err(err) => return err,
            },
            _ => None,
        };
        let limit = query.limit.unwrap_or(100);
        match state
            .db
            .list_app_secrets(&app_id, connection_id.as_ref(), limit)
            .await
        {
            Ok(secrets) => (StatusCode::OK, Json(json!({ "secrets": secrets }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_app_secret(
        State(state): State<AppState>,
        AxumPath((app_id, secret_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let secret_id = match parse_uri_field("secret_id", &secret_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_app_secret(&app_id, &secret_id).await {
            Ok(Some(secret)) => (StatusCode::OK, Json(json!({ "secret": secret }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "secret not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn upsert_app_secret(
        State(state): State<AppState>,
        AxumPath((app_id, secret_id)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertAppSecretRequest>,
    ) -> impl IntoResponse {
        if payload.key.trim().is_empty() {
            return api_error(StatusCode::BAD_REQUEST, "key must not be empty".to_string());
        }
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let secret_id = match parse_uri_field("secret_id", &secret_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let connection_id = match payload.connection_id {
            Some(raw) if !raw.trim().is_empty() => match parse_uri_field("connection_id", &raw) {
                Ok(value) => Some(value),
                Err(err) => return err,
            },
            _ => None,
        };
        match state
            .db
            .upsert_app_secret(
                &app_id,
                &secret_id,
                connection_id.as_ref(),
                payload.key.trim(),
                payload.value.as_str(),
                payload.kind.trim(),
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_app_secret(
        State(state): State<AppState>,
        AxumPath((app_id, secret_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let app_id = match parse_uri_field("app_id", &app_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let secret_id = match parse_uri_field("secret_id", &secret_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_app_secret(&app_id, &secret_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "secret not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}
