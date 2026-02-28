use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use borg_core::Config;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, warn};

use crate::AppState;
use crate::controllers::common::{api_error, parse_uri_field};

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct SessionsQuery {
    limit: Option<usize>,
    port: Option<String>,
    user: Option<String>,
    users: Option<String>,
    user_key: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PortSettingsQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct PortBindingsQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPortRequest {
    provider: String,
    enabled: bool,
    allows_guests: bool,
    #[serde(default)]
    default_agent_id: Option<String>,
    #[serde(default)]
    settings: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertProviderRequest {
    api_key: String,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPolicyRequest {
    policy: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAgentSpecRequest {
    #[serde(default)]
    name: Option<String>,
    model: String,
    system_prompt: String,
    tools: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertUserRequest {
    user_key: String,
    profile: Value,
}

#[derive(Deserialize)]
pub(crate) struct PatchUserRequest {
    profile: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertSessionRequest {
    session_id: String,
    users: Vec<String>,
    port: String,
}

#[derive(Deserialize)]
pub(crate) struct PatchSessionRequest {
    users: Option<Vec<String>>,
    port: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionMessagesQuery {
    from: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertSessionMessageRequest {
    payload: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPortSettingRequest {
    value: String,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPortBindingRequest {
    session_id: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPortSessionContextRequest {
    ctx: Value,
}

pub(crate) struct DbController;

impl DbController {
    fn default_models_for_provider(provider: &str) -> &'static [&'static str] {
        match provider {
            "openai" => &["gpt-5.3-codex", "gpt-4o-mini"],
            "openrouter" => &["openai/gpt-4o-mini", "meta-llama/3.3-70b-instruct"],
            _ => &[],
        }
    }

    fn fallback_agent_name(agent_id: &str) -> String {
        if agent_id == "borg:agent:default" {
            return "Default Agent".to_string();
        }
        let tail = agent_id.rsplit(':').next().unwrap_or(agent_id);
        if tail.is_empty() {
            "Agent".to_string()
        } else {
            tail.to_string()
        }
    }

    fn port_name_from_uri(port_uri: &str) -> Result<String, Response> {
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
        Ok(id.to_string())
    }

    pub(crate) async fn list_llm_calls(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(500);
        match state.db.list_llm_calls(limit).await {
            Ok(calls) => (StatusCode::OK, Json(json!({ "llm_calls": calls }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_llm_call(
        State(state): State<AppState>,
        AxumPath(call_id): AxumPath<String>,
    ) -> impl IntoResponse {
        match state.db.get_llm_call(&call_id).await {
            Ok(Some(call)) => (StatusCode::OK, Json(json!({ "llm_call": call }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "llm call not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn list_providers(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_providers(limit).await {
            Ok(providers) => {
                (StatusCode::OK, Json(json!({ "providers": providers }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_provider(
        State(state): State<AppState>,
        AxumPath(provider): AxumPath<String>,
    ) -> impl IntoResponse {
        match state.db.get_provider(&provider).await {
            Ok(Some(found)) => (StatusCode::OK, Json(json!({ "provider": found }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "provider not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn upsert_provider(
        State(state): State<AppState>,
        AxumPath(provider): AxumPath<String>,
        Json(payload): Json<UpsertProviderRequest>,
    ) -> impl IntoResponse {
        match state
            .db
            .upsert_provider(&provider, &payload.api_key, payload.enabled)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_provider(
        State(state): State<AppState>,
        AxumPath(provider): AxumPath<String>,
    ) -> impl IntoResponse {
        match state.db.delete_provider(&provider).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "provider not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn start_openai_device_code() -> impl IntoResponse {
        let config = Config::default();
        let Some(client_id) = config.openai_oauth_client_id else {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "openai_oauth_client_id is not configured in borg-core Config".to_string(),
            );
        };

        let response = match Client::new()
            .post(&config.openai_device_code_url)
            .form(&[
                ("client_id", client_id.as_str()),
                ("scope", config.openai_device_code_scope.as_str()),
            ])
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    format!("failed to reach OpenAI device-code endpoint: {err}"),
                );
            }
        };

        let status = response.status();
        let payload = match response.json::<Value>().await {
            Ok(payload) => payload,
            Err(err) => {
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    format!("invalid JSON from OpenAI device-code endpoint: {err}"),
                );
            }
        };

        if !status.is_success() {
            return api_error(
                StatusCode::BAD_GATEWAY,
                format!("OpenAI device-code start failed: status={status} body={payload}"),
            );
        }

        (
            StatusCode::OK,
            Json(json!({ "ok": true, "device_code": payload })),
        )
            .into_response()
    }

    pub(crate) async fn list_provider_models(
        AxumPath(provider): AxumPath<String>,
    ) -> impl IntoResponse {
        let models = Self::default_models_for_provider(&provider);
        if models.is_empty() {
            return api_error(StatusCode::NOT_FOUND, "provider not found".to_string());
        }
        (
            StatusCode::OK,
            Json(json!({
                "provider": provider,
                "models": models,
            })),
        )
            .into_response()
    }

    pub(crate) async fn list_policies(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_policies(limit).await {
            Ok(policies) => (StatusCode::OK, Json(json!({ "policies": policies }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_policy(
        State(state): State<AppState>,
        AxumPath(policy_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_policy(&policy_id).await {
            Ok(Some(policy)) => (StatusCode::OK, Json(json!({ "policy": policy }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_policy(
        State(state): State<AppState>,
        AxumPath(policy_id): AxumPath<String>,
        Json(payload): Json<UpsertPolicyRequest>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.upsert_policy(&policy_id, &payload.policy).await {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_policy(
        State(state): State<AppState>,
        AxumPath(policy_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_policy(&policy_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_policy_uses(
        State(state): State<AppState>,
        AxumPath(policy_id): AxumPath<String>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_policy(&policy_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        let limit = query.limit.unwrap_or(200);
        match state.db.list_policy_uses(&policy_id, limit).await {
            Ok(uses) => (StatusCode::OK, Json(json!({ "uses": uses }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn attach_policy_to_entity(
        State(state): State<AppState>,
        AxumPath((policy_id, entity_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let entity_id = match parse_uri_field("entity_id", &entity_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_policy(&policy_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "policy not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        match state
            .db
            .attach_policy_to_entity(&policy_id, &entity_id)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn detach_policy_from_entity(
        State(state): State<AppState>,
        AxumPath((policy_id, entity_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let policy_id = match parse_uri_field("policy_id", &policy_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let entity_id = match parse_uri_field("entity_id", &entity_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .detach_policy_from_entity(&policy_id, &entity_id)
            .await
        {
            Ok(0) => api_error(
                StatusCode::NOT_FOUND,
                "policy association not found".to_string(),
            ),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_agent_specs(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_agent_specs(limit).await {
            Ok(specs) => (StatusCode::OK, Json(json!({ "agent_specs": specs }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_agent_spec(
        State(state): State<AppState>,
        AxumPath(agent_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let agent_id = match parse_uri_field("agent_id", &agent_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_agent_spec(&agent_id).await {
            Ok(Some(spec)) => (StatusCode::OK, Json(json!({ "agent_spec": spec }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "agent spec not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_agent_spec(
        State(state): State<AppState>,
        AxumPath(agent_id): AxumPath<String>,
        Json(payload): Json<UpsertAgentSpecRequest>,
    ) -> impl IntoResponse {
        let agent_id = match parse_uri_field("agent_id", &agent_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let name = payload
            .name
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Self::fallback_agent_name(&agent_id.to_string()));
        match state
            .db
            .upsert_agent_spec(
                &agent_id,
                &name,
                &payload.model,
                &payload.system_prompt,
                &payload.tools,
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_agent_spec(
        State(state): State<AppState>,
        AxumPath(agent_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let agent_id = match parse_uri_field("agent_id", &agent_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_agent_spec(&agent_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "agent spec not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_users(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_users(limit).await {
            Ok(users) => (StatusCode::OK, Json(json!({ "users": users }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_user(
        State(state): State<AppState>,
        Json(payload): Json<UpsertUserRequest>,
    ) -> impl IntoResponse {
        let user_key = match parse_uri_field("user_key", &payload.user_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.upsert_user(&user_key, &payload.profile).await {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_user(
        State(state): State<AppState>,
        AxumPath(user_key): AxumPath<String>,
    ) -> impl IntoResponse {
        let user_key = match parse_uri_field("user_key", &user_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_user(&user_key).await {
            Ok(Some(user)) => (StatusCode::OK, Json(json!({ "user": user }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "user not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn patch_user(
        State(state): State<AppState>,
        AxumPath(user_key): AxumPath<String>,
        Json(payload): Json<PatchUserRequest>,
    ) -> impl IntoResponse {
        let user_key = match parse_uri_field("user_key", &user_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.upsert_user(&user_key, &payload.profile).await {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_user(
        State(state): State<AppState>,
        AxumPath(user_key): AxumPath<String>,
    ) -> impl IntoResponse {
        let user_key = match parse_uri_field("user_key", &user_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_user(&user_key).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "user not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_sessions(
        State(state): State<AppState>,
        Query(query): Query<SessionsQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        let raw_user_filter = query
            .users
            .clone()
            .or(query.user.clone())
            .or(query.user_key.clone());
        let raw_port = query.port.clone();
        debug!(
            target: "borg_api",
            limit,
            port = ?raw_port,
            user = ?raw_user_filter,
            "list_sessions request received"
        );
        let user_key = match raw_user_filter.as_deref() {
            Some(raw) => match parse_uri_field("user_key", raw) {
                Ok(v) => Some(v),
                Err(err) => {
                    warn!(
                        target: "borg_api",
                        user = raw,
                        "list_sessions rejected invalid user_key filter"
                    );
                    return err;
                }
            },
            None => None,
        };
        let port = match query.port.as_deref() {
            Some(raw) => match parse_uri_field("port", raw) {
                Ok(v) => Some(v),
                Err(err) => return err,
            },
            None => None,
        };
        match state
            .db
            .list_sessions(limit, port.as_ref(), user_key.as_ref())
            .await
        {
            Ok(sessions) => {
                debug!(
                    target: "borg_api",
                    count = sessions.len(),
                    limit,
                    port = ?raw_port,
                    user = ?raw_user_filter,
                    "list_sessions request completed"
                );
                (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response()
            }
            Err(err) => {
                warn!(
                    target: "borg_api",
                    error = %err,
                    limit,
                    port = ?raw_port,
                    user = ?raw_user_filter,
                    "list_sessions failed"
                );
                api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        }
    }

    pub(crate) async fn upsert_session(
        State(state): State<AppState>,
        Json(payload): Json<UpsertSessionRequest>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &payload.session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let port = match parse_uri_field("port", &payload.port) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let users = match payload
            .users
            .iter()
            .map(|value| parse_uri_field("users[]", value))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(value) if !value.is_empty() => value,
            Ok(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "users must not be empty".to_string(),
                );
            }
            Err(err) => return err,
        };
        match state.db.upsert_session(&session_id, &users, &port).await {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_session(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_session(&session_id).await {
            Ok(Some(session)) => {
                (StatusCode::OK, Json(json!({ "session": session }))).into_response()
            }
            Ok(None) => api_error(StatusCode::NOT_FOUND, "session not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn patch_session(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
        Json(payload): Json<PatchSessionRequest>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let Some(existing) = (match state.db.get_session(&session_id).await {
            Ok(v) => v,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }) else {
            return api_error(StatusCode::NOT_FOUND, "session not found".to_string());
        };

        let users = match payload.users {
            Some(raw_users) => match raw_users
                .iter()
                .map(|value| parse_uri_field("users[]", value))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(value) if !value.is_empty() => value,
                Ok(_) => {
                    return api_error(
                        StatusCode::BAD_REQUEST,
                        "users must not be empty".to_string(),
                    );
                }
                Err(err) => return err,
            },
            None => existing.users,
        };
        let port = match payload.port {
            Some(raw) => match parse_uri_field("port", &raw) {
                Ok(v) => v,
                Err(err) => return err,
            },
            None => existing.port,
        };

        match state.db.upsert_session(&session_id, &users, &port).await {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_session(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_session(&session_id).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "session not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn append_session_message(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
        Json(payload): Json<UpsertSessionMessageRequest>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .append_session_message(&session_id, &payload.payload)
            .await
        {
            Ok(message_index) => (
                StatusCode::OK,
                Json(json!({ "message_index": message_index })),
            )
                .into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_session_messages(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
        Query(query): Query<SessionMessagesQuery>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let from = query.from.unwrap_or(0);
        let limit = query.limit.unwrap_or(100);
        match state
            .db
            .list_session_messages(&session_id, from, limit)
            .await
        {
            Ok(messages) => (StatusCode::OK, Json(json!({ "messages": messages }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_session_message(
        State(state): State<AppState>,
        AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .get_session_message(&session_id, message_index)
            .await
        {
            Ok(Some(message)) => {
                (StatusCode::OK, Json(json!({ "message": message }))).into_response()
            }
            Ok(None) => api_error(
                StatusCode::NOT_FOUND,
                "session message not found".to_string(),
            ),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn patch_session_message(
        State(state): State<AppState>,
        AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
        Json(payload): Json<UpsertSessionMessageRequest>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .update_session_message(&session_id, message_index, &payload.payload)
            .await
        {
            Ok(0) => api_error(
                StatusCode::NOT_FOUND,
                "session message not found".to_string(),
            ),
            Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_session_message(
        State(state): State<AppState>,
        AxumPath((session_id, message_index)): AxumPath<(String, i64)>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .delete_session_message(&session_id, message_index)
            .await
        {
            Ok(0) => api_error(
                StatusCode::NOT_FOUND,
                "session message not found".to_string(),
            ),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn clear_session_messages(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.clear_session_history(&session_id).await {
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_ports(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(200);
        match state.db.list_ports(limit).await {
            Ok(ports) => (StatusCode::OK, Json(json!({ "ports": ports }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Json(payload): Json<UpsertPortRequest>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let default_agent_id = match payload.default_agent_id {
            Some(raw) if raw.trim().is_empty() => None,
            Some(raw) => match parse_uri_field("default_agent_id", raw.trim()) {
                Ok(uri) => Some(uri),
                Err(err) => return err,
            },
            None => None,
        };
        let settings = payload.settings.unwrap_or_else(|| json!({}));

        match state
            .db
            .upsert_port(
                &port_name,
                &payload.provider,
                payload.enabled,
                payload.allows_guests,
                default_agent_id.as_ref(),
                &settings,
            )
            .await
        {
            Ok(()) => match state
                .ports_supervisor
                .on_port_setting_changed(&port_name, "provider")
                .await
            {
                Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
                Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_port_settings(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Query(query): Query<PortSettingsQuery>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let limit = query.limit.unwrap_or(200);
        match state.db.list_port_settings(&port_name, limit).await {
            Ok(items) => {
                let settings: Vec<Value> = items
                    .into_iter()
                    .map(|(key, value)| json!({ "key": key, "value": value }))
                    .collect();
                (StatusCode::OK, Json(json!({ "settings": settings }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state.db.delete_port(&port_name).await {
            Ok(()) => match state
                .ports_supervisor
                .on_port_setting_changed(&port_name, "enabled")
                .await
            {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_setting(
        State(state): State<AppState>,
        AxumPath((port_uri, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state.db.get_port_setting(&port_name, &key).await {
            Ok(Some(value)) => (
                StatusCode::OK,
                Json(json!({ "port": port_name, "key": key, "value": value })),
            )
                .into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port_setting(
        State(state): State<AppState>,
        AxumPath((port_uri, key)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortSettingRequest>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state
            .db
            .upsert_port_setting(&port_name, &key, &payload.value)
            .await
        {
            Ok(()) => match state
                .ports_supervisor
                .on_port_setting_changed(&port_name, &key)
                .await
            {
                Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
                Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_setting(
        State(state): State<AppState>,
        AxumPath((port_uri, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state.db.delete_port_setting(&port_name, &key).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
            Ok(_) => match state
                .ports_supervisor
                .on_port_setting_changed(&port_name, &key)
                .await
            {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_port_bindings(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Query(query): Query<PortBindingsQuery>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let limit = query.limit.unwrap_or(200);
        match state.db.list_port_bindings(&port_name, limit).await {
            Ok(items) => {
                let bindings: Vec<Value> = items
                    .into_iter()
                    .map(|(conversation_key, session_id, agent_id)| {
                        json!({
                            "conversation_key": conversation_key,
                            "session_id": session_id,
                            "agent_id": agent_id
                        })
                    })
                    .collect();
                (StatusCode::OK, Json(json!({ "bindings": bindings }))).into_response()
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .get_port_binding_record(&port_name, &conversation_key)
            .await
        {
            Ok(Some((conversation_key, session_id, agent_id))) => (
                StatusCode::OK,
                Json(json!({
                    "binding": {
                        "conversation_key": conversation_key,
                        "session_id": session_id,
                        "agent_id": agent_id
                    }
                })),
            )
                .into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "port binding not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortBindingRequest>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let session_id = match parse_uri_field("session_id", &payload.session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let agent_id = match payload.agent_id {
            Some(raw) => match parse_uri_field("agent_id", &raw) {
                Ok(v) => Some(v),
                Err(err) => return err,
            },
            None => None,
        };

        match state
            .db
            .upsert_port_binding_record(
                &port_name,
                &conversation_key,
                &session_id,
                agent_id.as_ref(),
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_binding(
        State(state): State<AppState>,
        AxumPath((port_uri, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .delete_port_binding(&port_name, &conversation_key)
            .await
        {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "port binding not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_session_context(
        State(state): State<AppState>,
        AxumPath((port_uri, session_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .get_port_session_context(&port_name, &session_id)
            .await
        {
            Ok(Some(ctx)) => (
                StatusCode::OK,
                Json(json!({ "port": port_name, "session_id": session_id, "ctx": ctx })),
            )
                .into_response(),
            Ok(None) => api_error(
                StatusCode::NOT_FOUND,
                "port session context not found".to_string(),
            ),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port_session_context(
        State(state): State<AppState>,
        AxumPath((port_uri, session_id)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortSessionContextRequest>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .upsert_port_session_context(&port_name, &session_id, &payload.ctx)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_session_context(
        State(state): State<AppState>,
        AxumPath((port_uri, session_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&port_uri) {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .clear_port_session_context(&port_name, &session_id)
            .await
        {
            Ok(0) => api_error(
                StatusCode::NOT_FOUND,
                "port session context not found".to_string(),
            ),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_any_port_session_context(
        State(state): State<AppState>,
        AxumPath(session_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_any_port_session_context(&session_id).await {
            Ok(Some((port, ctx))) => (
                StatusCode::OK,
                Json(json!({ "port": port, "session_id": session_id, "ctx": ctx })),
            )
                .into_response(),
            Ok(None) => api_error(
                StatusCode::NOT_FOUND,
                "session context not found".to_string(),
            ),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }
}
