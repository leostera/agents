use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use borg_core::Config;
use borg_llm::Provider;
use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::providers::openrouter::OpenRouterProvider;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::controllers::common::api_error;

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertProviderRequest {
    #[serde(default)]
    provider_kind: Option<String>,
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

pub(crate) struct ProvidersController;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedProviderKind {
    OpenAi,
    OpenRouter,
    LmStudio,
    Ollama,
}

impl ProvidersController {
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

        let existing = match state.db.get_provider(&provider).await {
            Ok(value) => value,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let provider_kind = payload
            .provider_kind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
            .or_else(|| {
                Self::parse_provider_kind(&provider).map(|kind| match kind {
                    SupportedProviderKind::OpenAi => "openai".to_string(),
                    SupportedProviderKind::OpenRouter => "openrouter".to_string(),
                    SupportedProviderKind::LmStudio => "lmstudio".to_string(),
                    SupportedProviderKind::Ollama => "ollama".to_string(),
                })
            })
            .or_else(|| existing.as_ref().map(|record| record.provider_kind.clone()));
        let Some(provider_kind) = provider_kind else {
            return api_error(
                StatusCode::BAD_REQUEST,
                "provider_kind is required when creating a provider".to_string(),
            );
        };
        let Some(kind) = Self::parse_provider_kind(&provider_kind) else {
            return api_error(StatusCode::BAD_REQUEST, "unsupported provider_kind".to_string());
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
            .upsert_provider_with_kind(
                &provider,
                &provider_kind,
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
        let provider = provider.trim().to_ascii_lowercase();
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

        let Some(kind) = Self::parse_provider_kind(&configured_provider.provider_kind) else {
            return api_error(StatusCode::BAD_REQUEST, "unsupported provider_kind".to_string());
        };

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
}
