use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use axum::{
    Json,
    extract::{FromRef, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use borg_core::{Uri, uri};
use borg_db::{AppRecord, BorgDb};
use chrono::Utc;
use oauth2::url::form_urlencoded;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenUrl,
    basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::error;

use crate::catalog::DefaultAppsCatalog;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OAuthStartRequest {
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub return_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OAuthStartResponse {
    ok: bool,
    app_id: String,
    provider: String,
    connection_id: String,
    state: String,
    authorize_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Clone)]
struct OAuthAppConfig {
    authorize_url: String,
    token_url: String,
    client_id: String,
    client_secret: Option<String>,
    scopes: Vec<String>,
    redirect_uri: Option<String>,
    userinfo_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct OAuthTokenData {
    access_token: String,
    refresh_token: Option<String>,
    scope: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct OAuthTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
    expires_in: Option<u64>,
}

pub struct OAuthService {
    db: BorgDb,
}

impl OAuthService {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    async fn start(
        &self,
        app_id: &Uri,
        request: OAuthStartRequest,
        headers: &HeaderMap,
    ) -> Result<OAuthStartResponse> {
        let app = self.resolve_app(app_id).await?;
        let config = Self::oauth_config_for_app(&app)?;
        let redirect_uri = self.resolve_redirect_uri(&app, &config, headers)?;
        let client = Self::oauth_client(&config, &redirect_uri)?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut auth_request = client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_challenge);
        for scope in &config.scopes {
            auth_request = auth_request.add_scope(Scope::new(scope.clone()));
        }
        let (authorize_url, csrf_token) = auth_request.url();

        let owner_user_id = request
            .owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(Uri::parse)
            .transpose()?;
        let connection_id = uri!("borg", "app-connection");
        let now = Utc::now().to_rfc3339();
        let connection_json = json!({
            "oauth_state": csrf_token.secret(),
            "oauth_pkce_verifier": pkce_verifier.secret(),
            "oauth_started_at": now,
            "oauth_provider": app.slug,
            "return_to": request.return_to.as_deref().unwrap_or("").trim(),
        });
        self.db
            .upsert_app_connection(
                &app.app_id,
                &connection_id,
                owner_user_id.as_ref(),
                None,
                None,
                "pending_oauth",
                &connection_json,
            )
            .await?;

        Ok(OAuthStartResponse {
            ok: true,
            app_id: app.app_id.to_string(),
            provider: app.slug,
            connection_id: connection_id.to_string(),
            state: csrf_token.secret().to_string(),
            authorize_url: authorize_url.to_string(),
        })
    }

    async fn callback(
        &self,
        provider: &str,
        query: OAuthCallbackQuery,
        headers: &HeaderMap,
    ) -> Result<OAuthCallbackResult> {
        let app = self
            .db
            .get_app_by_slug(provider)
            .await?
            .ok_or_else(|| anyhow!("oauth provider app not found"))?;
        let config = Self::oauth_config_for_app(&app)?;

        if let Some(error_code) = query.error {
            let description = query.error_description.unwrap_or_default();
            return Err(anyhow!("oauth callback error: {error_code} {description}"));
        }

        let code = query
            .code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing oauth callback code"))?;
        let state = query
            .state
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing oauth callback state"))?;

        let connection = self
            .db
            .find_app_connection_by_oauth_state(&app.app_id, state)
            .await?
            .ok_or_else(|| anyhow!("oauth state is invalid or expired"))?;

        let redirect_uri = self.resolve_redirect_uri(&app, &config, headers)?;
        let pkce_verifier = connection
            .connection_json
            .get("oauth_pkce_verifier")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let token = self
            .exchange_token(&config, code, &redirect_uri, pkce_verifier.as_deref())
            .await?;

        let access_token = token.access_token.clone();
        self.upsert_connection_secret(
            &app.app_id,
            &connection.connection_id,
            "access_token",
            &access_token,
            "oauth",
        )
        .await?;
        if let Some(refresh_token) = token.refresh_token.as_deref() {
            self.upsert_connection_secret(
                &app.app_id,
                &connection.connection_id,
                "refresh_token",
                refresh_token,
                "oauth",
            )
            .await?;
        }
        if let Some(scope_value) = token
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.upsert_connection_secret(
                &app.app_id,
                &connection.connection_id,
                "scope",
                scope_value,
                "oauth",
            )
            .await?;
        }
        if let Some(expiry_seconds) = token.expires_in {
            let expires_at = self.oauth_expiry_timestamp(Duration::from_secs(expiry_seconds))?;
            self.upsert_connection_secret(
                &app.app_id,
                &connection.connection_id,
                "expires_at",
                &expires_at,
                "oauth",
            )
            .await?;
        }

        let profile = self
            .fetch_userinfo(config.userinfo_url.as_deref(), &access_token)
            .await?;
        let provider_account_id = Self::provider_account_id_from_profile(profile.as_ref());
        let external_user_id = Self::external_user_id_from_profile(profile.as_ref());

        let mut connection_json = connection.connection_json.clone();
        Self::set_connection_json_field(
            &mut connection_json,
            "connected_at",
            Value::String(Utc::now().to_rfc3339()),
        );
        Self::set_connection_json_field(
            &mut connection_json,
            "oauth_state",
            Value::String(String::new()),
        );
        Self::set_connection_json_field(
            &mut connection_json,
            "oauth_pkce_verifier",
            Value::String(String::new()),
        );
        if let Some(profile) = profile {
            Self::set_connection_json_field(&mut connection_json, "profile", profile);
        }

        self.db
            .upsert_app_connection(
                &app.app_id,
                &connection.connection_id,
                connection.owner_user_id.as_ref(),
                provider_account_id.as_deref(),
                external_user_id.as_deref(),
                "connected",
                &connection_json,
            )
            .await?;

        let return_to = connection
            .connection_json
            .get("return_to")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        Ok(OAuthCallbackResult {
            app_id: app.app_id.to_string(),
            provider: app.slug,
            connection_id: connection.connection_id.to_string(),
            return_to,
        })
    }

    async fn resolve_app(&self, app_id: &Uri) -> Result<AppRecord> {
        if let Some(app) = self.db.get_app(app_id).await? {
            return Ok(app);
        }

        DefaultAppsCatalog::new().install_missing(&self.db).await?;

        self.db
            .get_app(app_id)
            .await?
            .ok_or_else(|| anyhow!("app not found"))
    }

    fn oauth_config_for_app(app: &AppRecord) -> Result<OAuthAppConfig> {
        if !app.auth_strategy.eq_ignore_ascii_case("oauth2") {
            return Err(anyhow!("app is not configured for oauth2"));
        }
        let object = app
            .auth_config_json
            .as_object()
            .ok_or_else(|| anyhow!("app auth_config_json must be an object"))?;
        let authorize_url = Self::required_config_string(object, "authorize_url")?;
        let token_url = Self::required_config_string(object, "token_url")?;
        let client_id = Self::required_config_string(object, "client_id")?;
        let client_secret = object
            .get("client_secret")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let redirect_uri = object
            .get("redirect_uri")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let userinfo_url = object
            .get("userinfo_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let scopes = object
            .get("scopes")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(OAuthAppConfig {
            authorize_url,
            token_url,
            client_id,
            client_secret,
            scopes,
            redirect_uri,
            userinfo_url,
        })
    }

    fn oauth_client(config: &OAuthAppConfig, redirect_uri: &str) -> Result<BasicClient> {
        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            config.client_secret.clone().map(ClientSecret::new),
            AuthUrl::new(config.authorize_url.clone())?,
            Some(TokenUrl::new(config.token_url.clone())?),
        )
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);
        Ok(client)
    }

    async fn exchange_token(
        &self,
        config: &OAuthAppConfig,
        code: &str,
        redirect_uri: &str,
        pkce_verifier: Option<&str>,
    ) -> Result<OAuthTokenData> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", config.client_id.clone()),
            ("redirect_uri", redirect_uri.to_string()),
        ];
        if let Some(client_secret) = config
            .client_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            form.push(("client_secret", client_secret.to_string()));
        }
        if let Some(verifier) = pkce_verifier
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            form.push(("code_verifier", verifier.to_string()));
        }

        let response = reqwest::Client::new()
            .post(&config.token_url)
            .header("Accept", "application/json")
            .header("User-Agent", "borg")
            .form(&form)
            .send()
            .await
            .context("failed sending oauth token exchange request")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed reading oauth token exchange response body")?;
        if !status.is_success() {
            return Err(anyhow!(
                "oauth token exchange failed: status={status} body={body}"
            ));
        }

        if let Ok(parsed) = serde_json::from_str::<OAuthTokenResponse>(&body)
            && let Some(access_token) = parsed.access_token
        {
            return Ok(OAuthTokenData {
                access_token,
                refresh_token: parsed.refresh_token,
                scope: parsed.scope,
                expires_in: parsed.expires_in,
            });
        }

        let mut access_token = None;
        let mut refresh_token = None;
        let mut scope = None;
        let mut expires_in = None;
        for (key, value) in form_urlencoded::parse(body.as_bytes()) {
            let key = key.into_owned();
            let value = value.into_owned();
            match key.as_str() {
                "access_token" => access_token = Some(value),
                "refresh_token" => refresh_token = Some(value),
                "scope" => scope = Some(value),
                "expires_in" => expires_in = value.parse::<u64>().ok(),
                _ => {}
            }
        }
        let Some(access_token) = access_token else {
            return Err(anyhow!(
                "failed to parse oauth token exchange response: {body}"
            ));
        };
        Ok(OAuthTokenData {
            access_token,
            refresh_token,
            scope,
            expires_in,
        })
    }

    fn resolve_redirect_uri(
        &self,
        app: &AppRecord,
        config: &OAuthAppConfig,
        headers: &HeaderMap,
    ) -> Result<String> {
        if let Some(redirect_uri) = &config.redirect_uri {
            return Ok(redirect_uri.clone());
        }

        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing host header"))?;
        let scheme = headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("http");
        Ok(format!("{scheme}://{host}/oauth/{}/callback", app.slug))
    }

    async fn upsert_connection_secret(
        &self,
        app_id: &Uri,
        connection_id: &Uri,
        key: &str,
        value: &str,
        kind: &str,
    ) -> Result<()> {
        let existing = self
            .db
            .list_app_secrets(app_id, Some(connection_id), 500)
            .await?;
        let secret_id = existing
            .iter()
            .find(|secret| secret.key == key)
            .map(|secret| secret.secret_id.clone())
            .unwrap_or_else(|| uri!("borg", "app-secret"));
        self.db
            .upsert_app_secret(app_id, &secret_id, Some(connection_id), key, value, kind)
            .await
    }

    async fn fetch_userinfo(
        &self,
        userinfo_url: Option<&str>,
        access_token: &str,
    ) -> Result<Option<Value>> {
        let Some(userinfo_url) = userinfo_url else {
            return Ok(None);
        };
        let response = reqwest::Client::new()
            .get(userinfo_url)
            .bearer_auth(access_token)
            .header("User-Agent", "borg")
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(err) => {
                error!(
                    target: "borg_apps",
                    error = %err,
                    "oauth userinfo request failed"
                );
                return Ok(None);
            }
        };
        if !response.status().is_success() {
            error!(
                target: "borg_apps",
                status = %response.status(),
                "oauth userinfo response was not successful"
            );
            return Ok(None);
        }
        match response.json::<Value>().await {
            Ok(profile) => Ok(Some(profile)),
            Err(err) => {
                error!(
                    target: "borg_apps",
                    error = %err,
                    "oauth userinfo payload was not valid JSON"
                );
                Ok(None)
            }
        }
    }

    fn provider_account_id_from_profile(profile: Option<&Value>) -> Option<String> {
        profile.and_then(|profile| {
            profile
                .get("id")
                .and_then(Self::json_scalar_to_string)
                .or_else(|| profile.get("login").and_then(Self::json_scalar_to_string))
        })
    }

    fn external_user_id_from_profile(profile: Option<&Value>) -> Option<String> {
        profile.and_then(|profile| {
            profile
                .get("login")
                .and_then(Self::json_scalar_to_string)
                .or_else(|| profile.get("id").and_then(Self::json_scalar_to_string))
        })
    }

    fn json_scalar_to_string(value: &Value) -> Option<String> {
        match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        }
    }

    fn oauth_expiry_timestamp(&self, expires_in: Duration) -> Result<String> {
        let expires_in =
            chrono::Duration::from_std(expires_in).context("oauth token expiry is out of range")?;
        Ok((Utc::now() + expires_in).to_rfc3339())
    }

    fn required_config_string(
        object: &serde_json::Map<String, Value>,
        key: &str,
    ) -> Result<String> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("missing app auth config `{key}`"))
    }

    fn set_connection_json_field(connection_json: &mut Value, key: &str, value: Value) {
        if !connection_json.is_object() {
            *connection_json = json!({});
        }
        if let Some(object) = connection_json.as_object_mut() {
            object.insert(key.to_string(), value);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct OAuthCallbackResult {
    app_id: String,
    provider: String,
    connection_id: String,
    return_to: Option<String>,
}

impl OAuthCallbackResult {
    fn response(self) -> Response {
        if let Some(location) = self
            .return_to
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Redirect::to(location).into_response();
        }
        (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "app_id": self.app_id,
                "provider": self.provider,
                "connection_id": self.connection_id
            })),
        )
            .into_response()
    }
}

pub async fn oauth_start<S>(
    State(state): State<S>,
    Path(app_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<OAuthStartRequest>,
) -> impl IntoResponse
where
    BorgDb: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    let app_id = match Uri::parse(&app_id) {
        Ok(app_id) => app_id,
        Err(err) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("invalid app_id uri: {err:#}"),
            );
        }
    };
    let service = OAuthService::new(BorgDb::from_ref(&state));
    match service.start(&app_id, payload, &headers).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

pub async fn oauth_provider_callback<S>(
    State(state): State<S>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallbackQuery>,
) -> impl IntoResponse
where
    BorgDb: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    let service = OAuthService::new(BorgDb::from_ref(&state));
    match service.callback(provider.trim(), query, &headers).await {
        Ok(result) => result.response(),
        Err(err) => api_error(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

fn api_error(status: StatusCode, message: String) -> Response {
    error!(
        target: "borg_apps",
        status = %status,
        error = %message,
        "oauth request failed"
    );
    (
        status,
        Json(json!({
            "ok": false,
            "error": message
        })),
    )
        .into_response()
}
