use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use borg_core::Config;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

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
pub(crate) struct UpsertProviderRequest {
    api_key: String,
}

#[derive(Deserialize)]
pub(crate) struct UpsertPolicyRequest {
    policy: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAgentSpecRequest {
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
    user_key: String,
    port: String,
    root_task_id: String,
    state: Value,
}

#[derive(Deserialize)]
pub(crate) struct PatchSessionRequest {
    user_key: Option<String>,
    port: Option<String>,
    root_task_id: Option<String>,
    state: Option<Value>,
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
            .upsert_provider_api_key(&provider, &payload.api_key)
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
        match state
            .db
            .upsert_agent_spec(
                &agent_id,
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
        let user_key = match query.user_key {
            Some(raw) => match parse_uri_field("user_key", &raw) {
                Ok(v) => Some(v),
                Err(err) => return err,
            },
            None => None,
        };
        match state
            .db
            .list_sessions(limit, query.port.as_deref(), user_key.as_ref())
            .await
        {
            Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
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
        let user_key = match parse_uri_field("user_key", &payload.user_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let root_task_id = match parse_uri_field("root_task_id", &payload.root_task_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .upsert_session(
                &session_id,
                &user_key,
                &payload.port,
                &root_task_id,
                &payload.state,
            )
            .await
        {
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

        let user_key = match payload.user_key {
            Some(raw) => match parse_uri_field("user_key", &raw) {
                Ok(v) => v,
                Err(err) => return err,
            },
            None => existing.user_key,
        };
        let root_task_id = match payload.root_task_id {
            Some(raw) => match parse_uri_field("root_task_id", &raw) {
                Ok(v) => v,
                Err(err) => return err,
            },
            None => existing.root_task_id,
        };
        let port = payload.port.unwrap_or(existing.port);
        let state_value = payload.state.unwrap_or(existing.state);

        match state
            .db
            .upsert_session(&session_id, &user_key, &port, &root_task_id, &state_value)
            .await
        {
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

    pub(crate) async fn list_port_settings(
        State(state): State<AppState>,
        AxumPath(port): AxumPath<String>,
        Query(query): Query<PortSettingsQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(200);
        match state.db.list_port_settings(&port, limit).await {
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

    pub(crate) async fn get_port_setting(
        State(state): State<AppState>,
        AxumPath((port, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        match state.db.get_port_setting(&port, &key).await {
            Ok(Some(value)) => (
                StatusCode::OK,
                Json(json!({ "port": port, "key": key, "value": value })),
            )
                .into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_port_setting(
        State(state): State<AppState>,
        AxumPath((port, key)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortSettingRequest>,
    ) -> impl IntoResponse {
        match state
            .db
            .upsert_port_setting(&port, &key, &payload.value)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_setting(
        State(state): State<AppState>,
        AxumPath((port, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        match state.db.delete_port_setting(&port, &key).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_port_bindings(
        State(state): State<AppState>,
        AxumPath(port): AxumPath<String>,
        Query(query): Query<PortBindingsQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(200);
        match state.db.list_port_bindings(&port, limit).await {
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
        AxumPath((port, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .get_port_binding_record(&port, &conversation_key)
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
        AxumPath((port, conversation_key)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortBindingRequest>,
    ) -> impl IntoResponse {
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
            .upsert_port_binding_record(&port, &conversation_key, &session_id, agent_id.as_ref())
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_binding(
        State(state): State<AppState>,
        AxumPath((port, conversation_key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let conversation_key = match parse_uri_field("conversation_key", &conversation_key) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.delete_port_binding(&port, &conversation_key).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "port binding not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_session_context(
        State(state): State<AppState>,
        AxumPath((port, session_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_port_session_context(&port, &session_id).await {
            Ok(Some(ctx)) => (
                StatusCode::OK,
                Json(json!({ "port": port, "session_id": session_id, "ctx": ctx })),
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
        AxumPath((port, session_id)): AxumPath<(String, String)>,
        Json(payload): Json<UpsertPortSessionContextRequest>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .upsert_port_session_context(&port, &session_id, &payload.ctx)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_session_context(
        State(state): State<AppState>,
        AxumPath((port, session_id)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let session_id = match parse_uri_field("session_id", &session_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .clear_port_session_context(&port, &session_id)
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
