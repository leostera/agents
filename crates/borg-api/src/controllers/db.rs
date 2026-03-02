use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use borg_core::Config;
use borg_core::TelegramUserId;
use borg_llm::Provider;
use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::providers::openrouter::OpenRouterProvider;
use borg_taskgraph::{
    CreateTaskInput as TaskGraphCreateTaskInput, ListParams as TaskGraphListParams, TaskGraphStore,
    TaskPatch as TaskGraphTaskPatch, TaskStatus,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, warn};

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
pub(crate) struct TaskGraphListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct CreateTaskGraphTaskRequest {
    session_uri: String,
    creator_agent_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    definition_of_done: String,
    assignee_agent_id: String,
    #[serde(default)]
    parent_uri: Option<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpdateTaskGraphTaskFieldsRequest {
    session_uri: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SetTaskGraphTaskStatusRequest {
    session_uri: String,
    status: String,
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
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    default_text_model: Option<String>,
    #[serde(default)]
    default_audio_model: Option<String>,
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

#[derive(Deserialize)]
pub(crate) struct UpsertPolicyRequest {
    policy: Value,
}

#[derive(Deserialize)]
pub(crate) struct UpsertAgentSpecRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    default_provider_id: Option<String>,
    model: String,
    system_prompt: String,
}

#[derive(Deserialize)]
pub(crate) struct SetAgentSpecEnabledRequest {
    enabled: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedProviderKind {
    OpenAi,
    OpenRouter,
    LmStudio,
    Ollama,
}

impl DbController {
    fn parse_provider_kind(raw: &str) -> Option<SupportedProviderKind> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" => Some(SupportedProviderKind::OpenAi),
            "openrouter" => Some(SupportedProviderKind::OpenRouter),
            "lmstudio" => Some(SupportedProviderKind::LmStudio),
            "ollama" => Some(SupportedProviderKind::Ollama),
            _ => None,
        }
    }

    fn normalize_optional_field(value: Option<&str>) -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn normalize_existing_api_key(value: &str) -> Option<String> {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    fn validate_provider_config(
        kind: SupportedProviderKind,
        provider: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<(), Response> {
        match kind {
            SupportedProviderKind::OpenAi | SupportedProviderKind::OpenRouter => {
                if api_key.is_none() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        format!("provider `{provider}` requires a non-empty api_key"),
                    ));
                }
            }
            SupportedProviderKind::LmStudio | SupportedProviderKind::Ollama => {
                if base_url.is_none() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        format!("provider `{provider}` requires a non-empty base_url"),
                    ));
                }
            }
        }

        Ok(())
    }

    async fn fetch_openai_compatible_models(
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let base_url = base_url.trim_end_matches('/');
        let url = format!("{base_url}/v1/models");
        let client = Client::new();
        let mut request = client.get(&url);
        if let Some(api_key) = api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.map_err(|err| err.to_string())?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("models endpoint returned {status}: {body}"));
        }

        let payload = response
            .json::<Value>()
            .await
            .map_err(|err| err.to_string())?;
        let mut models = payload
            .get("data")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str))
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        models.sort();
        models.dedup();
        Ok(models)
    }

    fn validate_port_settings(provider: &str, settings: &Value) -> Result<(), Response> {
        let normalized = provider.trim().to_ascii_lowercase();
        let Some(value) = settings
            .as_object()
            .and_then(|obj| obj.get("allowed_external_user_ids"))
        else {
            return Ok(());
        };

        let Some(items) = value.as_array() else {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{normalized}.allowed_external_user_ids must be an array"),
            ));
        };

        for item in items {
            let Some(raw) = item.as_str() else {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    format!("{normalized}.allowed_external_user_ids entries must be strings"),
                ));
            };
            if normalized == "telegram" && raw.parse::<TelegramUserId>().is_err() {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "telegram.allowed_external_user_ids entries must be numeric ids (e.g. 2654566) or usernames (e.g. @leostera)".to_string(),
                ));
            }
            if normalized == "discord" && !is_valid_discord_user_id(raw) {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "discord.allowed_external_user_ids entries must be numeric Discord user ids (snowflakes)".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn infer_default_text_model(models: &[String]) -> Option<String> {
        models
            .iter()
            .find(|model| !model.contains("transcribe"))
            .cloned()
    }

    fn infer_default_audio_model(models: &[String]) -> Option<String> {
        models
            .iter()
            .find(|model| model.contains("transcribe"))
            .cloned()
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
            Ok(Some(port)) => return Ok(port.port_name),
            Ok(None) => {}
            Err(err) => {
                return Err(api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("{err:#}"),
                ));
            }
        }

        Ok(id.to_string())
    }

    pub(crate) async fn list_tool_calls(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(500);
        match state.db.list_tool_calls(limit).await {
            Ok(calls) => (StatusCode::OK, Json(json!({ "tool_calls": calls }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn get_tool_call(
        State(state): State<AppState>,
        AxumPath(call_id): AxumPath<String>,
    ) -> impl IntoResponse {
        match state.db.get_tool_call(&call_id).await {
            Ok(Some(call)) => (StatusCode::OK, Json(json!({ "tool_call": call }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "tool call not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
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

    pub(crate) async fn list_taskgraph_tasks(
        State(state): State<AppState>,
        Query(query): Query<TaskGraphListQuery>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let params = TaskGraphListParams {
            cursor: query.cursor,
            limit: query.limit.unwrap_or(200),
        };
        match store.list_tasks(params).await {
            Ok((tasks, next_cursor)) => (
                StatusCode::OK,
                Json(json!({ "tasks": tasks, "next_cursor": next_cursor })),
            )
                .into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn create_taskgraph_task(
        State(state): State<AppState>,
        Json(payload): Json<CreateTaskGraphTaskRequest>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let input = TaskGraphCreateTaskInput {
            title: payload.title,
            description: payload.description,
            definition_of_done: payload.definition_of_done,
            assignee_agent_id: payload.assignee_agent_id,
            parent_uri: payload.parent_uri,
            blocked_by: payload.blocked_by,
            references: payload.references,
            labels: payload.labels,
        };
        match store
            .create_task(&payload.session_uri, &payload.creator_agent_id, input)
            .await
        {
            Ok(task) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
            Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
        }
    }

    pub(crate) async fn get_taskgraph_task(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        match store.get_task(&task_uri).await {
            Ok(task) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn update_taskgraph_task_fields(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
        Json(payload): Json<UpdateTaskGraphTaskFieldsRequest>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let patch = TaskGraphTaskPatch {
            title: payload.title,
            description: payload.description,
            definition_of_done: payload.definition_of_done,
        };
        match store
            .update_task_fields(&payload.session_uri, &task_uri, patch)
            .await
        {
            Ok(task) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
            Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
        }
    }

    pub(crate) async fn set_taskgraph_task_status(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
        Json(payload): Json<SetTaskGraphTaskStatusRequest>,
    ) -> impl IntoResponse {
        let Some(status) = TaskStatus::parse(payload.status.trim()) else {
            return api_error(
                StatusCode::BAD_REQUEST,
                "invalid status; expected pending|doing|review|done|discarded".to_string(),
            );
        };

        let store = TaskGraphStore::new(state.db.clone());
        match store
            .set_task_status(&payload.session_uri, &task_uri, status)
            .await
        {
            Ok(task) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
            Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
        }
    }

    pub(crate) async fn list_taskgraph_comments(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
        Query(query): Query<TaskGraphListQuery>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let params = TaskGraphListParams {
            cursor: query.cursor,
            limit: query.limit.unwrap_or(200),
        };
        match store.list_comments(&task_uri, params).await {
            Ok((comments, next_cursor)) => (
                StatusCode::OK,
                Json(json!({ "comments": comments, "next_cursor": next_cursor })),
            )
                .into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn list_taskgraph_events(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
        Query(query): Query<TaskGraphListQuery>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let params = TaskGraphListParams {
            cursor: query.cursor,
            limit: query.limit.unwrap_or(200),
        };
        match store.list_events(&task_uri, params).await {
            Ok((events, next_cursor)) => (
                StatusCode::OK,
                Json(json!({ "events": events, "next_cursor": next_cursor })),
            )
                .into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")),
        }
    }

    pub(crate) async fn list_taskgraph_children(
        State(state): State<AppState>,
        AxumPath(task_uri): AxumPath<String>,
        Query(query): Query<TaskGraphListQuery>,
    ) -> impl IntoResponse {
        let store = TaskGraphStore::new(state.db.clone());
        let params = TaskGraphListParams {
            cursor: query.cursor,
            limit: query.limit.unwrap_or(200),
        };
        match store.list_task_children(&task_uri, params).await {
            Ok((children, next_cursor)) => (
                StatusCode::OK,
                Json(json!({ "children": children, "next_cursor": next_cursor })),
            )
                .into_response(),
            Err(err) if err.to_string().contains("task.not_found") => {
                api_error(StatusCode::NOT_FOUND, "task not found".to_string())
            }
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
        let provider = provider.trim().to_ascii_lowercase();
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
        let provider = provider.trim().to_ascii_lowercase();
        let Some(kind) = Self::parse_provider_kind(&provider) else {
            return api_error(StatusCode::NOT_FOUND, "provider not found".to_string());
        };

        let existing = match state.db.get_provider(&provider).await {
            Ok(value) => value,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let api_key = Self::normalize_optional_field(payload.api_key.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|record| Self::normalize_existing_api_key(&record.api_key))
        });
        let base_url = Self::normalize_optional_field(payload.base_url.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|record| Self::normalize_optional_field(record.base_url.as_deref()))
        });

        if let Err(err) =
            Self::validate_provider_config(kind, &provider, api_key.as_deref(), base_url.as_deref())
        {
            return err;
        }

        match state
            .db
            .upsert_provider(
                &provider,
                api_key.as_deref(),
                base_url.as_deref(),
                payload.enabled,
                payload.default_text_model.as_deref(),
                payload.default_audio_model.as_deref(),
            )
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
        State(state): State<AppState>,
        AxumPath(provider): AxumPath<String>,
    ) -> impl IntoResponse {
        let provider = provider.trim().to_ascii_lowercase();
        let Some(kind) = Self::parse_provider_kind(&provider) else {
            return api_error(StatusCode::NOT_FOUND, "provider not found".to_string());
        };

        let Some(configured_provider) = (match state.db.get_provider(&provider).await {
            Ok(value) => value,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }) else {
            return api_error(
                StatusCode::NOT_FOUND,
                format!("provider `{provider}` is not configured or enabled"),
            );
        };

        if !configured_provider.enabled {
            return api_error(
                StatusCode::NOT_FOUND,
                format!("provider `{provider}` is not configured or enabled"),
            );
        }

        let api_key = Self::normalize_existing_api_key(&configured_provider.api_key);
        let base_url = Self::normalize_optional_field(configured_provider.base_url.as_deref());

        let models_result = match kind {
            SupportedProviderKind::OpenAi => {
                let Some(api_key) = api_key else {
                    return api_error(
                        StatusCode::BAD_REQUEST,
                        "provider `openai` requires a configured api_key".to_string(),
                    );
                };
                let mut builder = OpenAiProvider::build().api_key(api_key);
                if let Some(base_url) = &base_url {
                    builder = builder.base_url(base_url.clone());
                }
                let provider = match builder.build() {
                    Ok(provider) => provider,
                    Err(err) => return api_error(StatusCode::BAD_REQUEST, err.to_string()),
                };
                provider.available_models().await
            }
            SupportedProviderKind::OpenRouter => {
                let Some(api_key) = api_key else {
                    return api_error(
                        StatusCode::BAD_REQUEST,
                        "provider `openrouter` requires a configured api_key".to_string(),
                    );
                };
                let mut builder = OpenRouterProvider::build().api_key(api_key);
                if let Some(base_url) = &base_url {
                    builder = builder.base_url(base_url.clone());
                }
                let provider = match builder.build() {
                    Ok(provider) => provider,
                    Err(err) => return api_error(StatusCode::BAD_REQUEST, err.to_string()),
                };
                provider.available_models().await
            }
            SupportedProviderKind::LmStudio | SupportedProviderKind::Ollama => {
                let Some(base_url) = base_url else {
                    return api_error(
                        StatusCode::BAD_REQUEST,
                        format!("provider `{provider}` requires a configured base_url"),
                    );
                };
                Self::fetch_openai_compatible_models(&base_url, api_key.as_deref())
                    .await
                    .map_err(borg_llm::LlmError::message)
            }
        };
        let models = match models_result {
            Ok(models) => models,
            Err(err) => return api_error(StatusCode::BAD_GATEWAY, err.to_string()),
        };

        (
            StatusCode::OK,
            Json(json!({
                "provider": provider,
                "models": models,
                "default_text_model": Self::infer_default_text_model(&models),
                "default_audio_model": Self::infer_default_audio_model(&models),
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
        let default_provider_id = payload
            .default_provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match state
            .db
            .upsert_agent_spec(
                &agent_id,
                &name,
                default_provider_id,
                &payload.model,
                &payload.system_prompt,
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

    pub(crate) async fn set_agent_spec_enabled(
        State(state): State<AppState>,
        AxumPath(agent_id): AxumPath<String>,
        Json(payload): Json<SetAgentSpecEnabledRequest>,
    ) -> impl IntoResponse {
        let agent_id = match parse_uri_field("agent_id", &agent_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state
            .db
            .set_agent_spec_enabled(&agent_id, payload.enabled)
            .await
        {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "agent spec not found".to_string()),
            Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        if let Err(err) = Self::validate_port_settings(&payload.provider, &settings) {
            return err;
        }

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
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_port_settings(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Query(query): Query<PortSettingsQuery>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state.db.delete_port(&port_name).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_port_setting(
        State(state): State<AppState>,
        AxumPath((port_uri, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state
            .db
            .upsert_port_setting(&port_name, &key, &payload.value)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn delete_port_setting(
        State(state): State<AppState>,
        AxumPath((port_uri, key)): AxumPath<(String, String)>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
            Ok(port_name) => port_name,
            Err(err) => return err,
        };
        match state.db.delete_port_setting(&port_name, &key).await {
            Ok(0) => api_error(StatusCode::NOT_FOUND, "port setting not found".to_string()),
            Ok(_) => StatusCode::NO_CONTENT.into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_port_bindings(
        State(state): State<AppState>,
        AxumPath(port_uri): AxumPath<String>,
        Query(query): Query<PortBindingsQuery>,
    ) -> impl IntoResponse {
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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
        let port_name = match Self::port_name_from_uri(&state, &port_uri).await {
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

fn is_valid_discord_user_id(value: &str) -> bool {
    let trimmed = value.trim();
    let len = trimmed.len();
    (16..=21).contains(&len) && trimmed.chars().all(|ch| ch.is_ascii_digit())
}
