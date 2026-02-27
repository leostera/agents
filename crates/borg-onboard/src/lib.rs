use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use borg_db::BorgDb;
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tracing::info;

const LOOPBACK_ADDR: [u8; 4] = [127, 0, 0, 1];
const HEALTH_STATUS_OK: &str = "ok";
const OPENAI_PROVIDER: &str = "openai";
const OPENROUTER_PROVIDER: &str = "openrouter";
const RUNTIME_SETTINGS_PORT: &str = "runtime";
const RUNTIME_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";

const ONBOARD_DIST_DIR: &str = "packages/borg-app/dist";
const ONBOARD_HTML_FILE: &str = "index.html";
const ONBOARD_JS_FILE: &str = "assets/app.js";
const ONBOARD_CSS_FILE: &str = "assets/app.css";

#[derive(Debug, Clone)]
pub struct OnboardServer {
    addr: SocketAddr,
    config_db_path: PathBuf,
}

#[derive(Clone)]
struct OnboardState {
    db: BorgDb,
    assets: Arc<OnboardAssets>,
}

#[derive(Debug, Deserialize)]
struct ProviderKeyPayload {
    api_key: String,
}

#[derive(Clone)]
struct OnboardAssets {
    html: Arc<String>,
    app_js: Arc<Vec<u8>>,
    app_css: Arc<Vec<u8>>,
}

impl OnboardAssets {
    async fn load() -> Result<Self> {
        let dist_dir = workspace_root()?.join(ONBOARD_DIST_DIR);
        if !dist_dir.exists() {
            return Err(anyhow!(
                "missing onboarding dist directory: {} (run `bun run build:web`)",
                dist_dir.display()
            ));
        }

        let html_path = dist_dir.join(ONBOARD_HTML_FILE);
        let js_path = dist_dir.join(ONBOARD_JS_FILE);
        let css_path = dist_dir.join(ONBOARD_CSS_FILE);

        if !html_path.exists() {
            return Err(anyhow!(
                "missing onboarding html asset: {}",
                html_path.display()
            ));
        }

        if !js_path.exists() {
            return Err(anyhow!(
                "missing onboarding js asset: {}",
                js_path.display()
            ));
        }

        if !css_path.exists() {
            return Err(anyhow!(
                "missing onboarding css asset: {}",
                css_path.display()
            ));
        }

        info!(target: "borg_onboard", path = %dist_dir.display(), "loading onboarding assets");
        let html = tokio::fs::read_to_string(html_path).await?;
        let app_js = tokio::fs::read(js_path).await?;
        let app_css = tokio::fs::read(css_path).await?;

        Ok(Self {
            html: Arc::new(html),
            app_js: Arc::new(app_js),
            app_css: Arc::new(app_css),
        })
    }
}

impl OnboardServer {
    pub fn new(port: u16, config_db_path: PathBuf) -> Self {
        Self {
            addr: SocketAddr::from((LOOPBACK_ADDR, port)),
            config_db_path,
        }
    }

    pub async fn run(self) -> Result<()> {
        let assets = OnboardAssets::load().await?;
        let config_path = self.config_db_path.to_string_lossy().to_string();
        let db = BorgDb::open_local(&config_path).await?;
        db.migrate().await?;

        let state = OnboardState {
            db,
            assets: Arc::new(assets),
        };

        let app = Router::new()
            .route(
                "/health",
                get(|| async { Json(json!({ "status": HEALTH_STATUS_OK })) }),
            )
            .route("/", get(onboard_page))
            .route("/onboard", get(onboard_page))
            .route("/dashboard", get(onboard_page))
            .route("/assets/app.css", get(onboard_app_css))
            .route("/assets/app.js", get(onboard_app_js))
            .route("/api/providers/:provider", post(save_provider_key))
            .with_state(state);

        let listener = TcpListener::bind(self.addr).await?;
        info!(target: "borg_onboard", address = %self.addr, "onboarding web server listening");
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn onboard_page(State(state): State<OnboardState>) -> Html<String> {
    Html((*state.assets.html).clone())
}

async fn onboard_app_js(State(state): State<OnboardState>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        (*state.assets.app_js).clone(),
    )
}

async fn onboard_app_css(State(state): State<OnboardState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        (*state.assets.app_css).clone(),
    )
}

async fn save_provider_key(
    State(state): State<OnboardState>,
    Path(provider): Path<String>,
    Json(payload): Json<ProviderKeyPayload>,
) -> impl IntoResponse {
    let Some(provider) = supported_provider(provider.as_str()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported provider" })),
        )
            .into_response();
    };

    if payload.api_key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "api_key must not be empty" })),
        )
            .into_response();
    }

    match state
        .db
        .upsert_provider_api_key(provider, payload.api_key.trim())
        .await
    {
        Ok(()) => match state
            .db
            .upsert_port_setting(
                RUNTIME_SETTINGS_PORT,
                RUNTIME_PREFERRED_PROVIDER_KEY,
                provider,
            )
            .await
        {
            Ok(()) => (
                StatusCode::OK,
                Json(json!({ "ok": true, "provider": provider })),
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
                .into_response(),
        },
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

fn workspace_root() -> Result<PathBuf> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match crate_root.parent().and_then(|p| p.parent()) {
        Some(path) => Ok(path.to_path_buf()),
        None => Err(anyhow!("failed to resolve workspace root")),
    }
}

fn supported_provider(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        OPENAI_PROVIDER => Some(OPENAI_PROVIDER),
        OPENROUTER_PROVIDER => Some(OPENROUTER_PROVIDER),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::supported_provider;

    #[test]
    fn supported_provider_accepts_expected_values() {
        assert_eq!(supported_provider("openai"), Some("openai"));
        assert_eq!(supported_provider("openrouter"), Some("openrouter"));
        assert_eq!(supported_provider("  OPENROUTER  "), Some("openrouter"));
    }

    #[test]
    fn supported_provider_rejects_unknown_values() {
        assert_eq!(supported_provider(""), None);
        assert_eq!(supported_provider("anthropic"), None);
    }
}
