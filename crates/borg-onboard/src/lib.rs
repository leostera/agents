use std::{net::SocketAddr, path::PathBuf};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use borg_db::BorgDb;
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tracing::info;
use turso::Builder;

const LOOPBACK_ADDR: [u8; 4] = [127, 0, 0, 1];
const HEALTH_STATUS_OK: &str = "ok";
const OPENAI_PROVIDER: &str = "openai";

#[derive(Debug, Clone)]
pub struct OnboardServer {
    addr: SocketAddr,
    config_db_path: PathBuf,
}

#[derive(Clone)]
struct OnboardState {
    db: BorgDb,
}

#[derive(Debug, Deserialize)]
struct OpenAiKeyPayload {
    api_key: String,
}

impl OnboardServer {
    pub fn new(port: u16, config_db_path: PathBuf) -> Self {
        Self {
            addr: SocketAddr::from((LOOPBACK_ADDR, port)),
            config_db_path,
        }
    }

    pub async fn run(self) -> Result<()> {
        let config_path = self.config_db_path.to_string_lossy().to_string();
        let db_handle = Builder::new_local(&config_path).build().await?;
        let db = BorgDb::new(db_handle.connect()?);
        db.migrate().await?;

        let state = OnboardState { db };
        let app = Router::new()
            .route(
                "/health",
                get(|| async { Json(json!({ "status": HEALTH_STATUS_OK })) }),
            )
            .route("/onboard", get(onboard_page))
            .route("/assets/app.css", get(onboard_app_css))
            .route("/assets/app.js", get(onboard_app_js))
            .route("/api/providers/openai", post(save_openai_key))
            .with_state(state);

        let listener = TcpListener::bind(self.addr).await?;
        info!(target: "borg_onboard", address = %self.addr, "onboarding web server listening");
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn onboard_page() -> Html<&'static str> {
    Html(ONBOARD_HTML)
}

async fn onboard_app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        ONBOARD_APP_JS,
    )
}

async fn onboard_app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        ONBOARD_APP_CSS,
    )
}

async fn save_openai_key(
    State(state): State<OnboardState>,
    Json(payload): Json<OpenAiKeyPayload>,
) -> impl IntoResponse {
    if payload.api_key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "api_key must not be empty" })),
        )
            .into_response();
    }

    match state
        .db
        .upsert_provider_api_key(OPENAI_PROVIDER, payload.api_key.trim())
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "ok": true, "provider": OPENAI_PROVIDER })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

const ONBOARD_HTML: &str = r#"<!doctype html>
<html lang=\"en\">
  <head>
    <meta charset=\"utf-8\" />
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
    <title>Borg Onboarding</title>
    <link rel=\"stylesheet\" href=\"/assets/app.css\" />
  </head>
  <body class=\"onboard-body\">
    <main class=\"onboard-main\">
      <h1 class=\"onboard-title\">Borg Onboarding</h1>
      <p class=\"onboard-subtitle\">Configure your first provider to start Borg.</p>
      <div id=\"app\" class=\"onboard-app\"></div>
    </main>
    <script type=\"module\" src=\"/assets/app.js\"></script>
  </body>
</html>
"#;

const ONBOARD_APP_JS: &str = r#"import { Effect, pipe } from 'https://esm.sh/effect@3.13.5';

const app = document.getElementById('app');
const state = {
  step: 1,
  provider: 'OpenAI',
  apiKey: '',
  loading: false,
  error: '',
  saved: false,
};

function render() {
  if (!app) return;

  if (state.step === 1) {
    app.innerHTML = `
      <section class=\"card\">
        <p class=\"step\">Step 1 of 2</p>
        <h2 class=\"card-title\">Choose LLM Provider</h2>
        <button id=\"choose-openai\" class=\"btn-provider\">
          <span class=\"btn-provider-title\">OpenAI (API key)</span>
          <span class=\"btn-provider-note\">Currently the only provider supported in onboarding.</span>
        </button>
      </section>
    `;

    document.getElementById('choose-openai')?.addEventListener('click', () => {
      state.step = 2;
      render();
    });
    return;
  }

  app.innerHTML = `
    <section class=\"card\">
      <p class=\"step\">Step 2 of 2</p>
      <h2 class=\"card-title\">Enter OpenAI API Key</h2>
      <p class=\"card-note\">This will be stored in <code>~/.borg/config.db</code> under <code>providers</code>.</p>
      <label class=\"field-label\">API Key</label>
      <input id=\"api-key\" type=\"password\" placeholder=\"sk-...\" class=\"field-input\" value=\"${state.apiKey}\" />
      ${state.error ? `<p class=\"notice-error\">${state.error}</p>` : ''}
      ${state.saved ? `<p class=\"notice-success\">Saved. You can now run <code>borg start</code>.</p>` : ''}
      <div class=\"actions\">
        <button id=\"back\" class=\"btn-secondary\">Back</button>
        <button id=\"save\" class=\"btn-primary\" ${state.loading ? 'disabled' : ''}>
          ${state.loading ? 'Saving...' : 'Save'}
        </button>
      </div>
    </section>
  `;

  document.getElementById('api-key')?.addEventListener('input', (ev) => {
    state.apiKey = ev.target.value;
  });

  document.getElementById('back')?.addEventListener('click', () => {
    state.step = 1;
    state.error = '';
    state.saved = false;
    render();
  });

  document.getElementById('save')?.addEventListener('click', () => {
    state.error = '';
    state.saved = false;
    state.loading = true;
    render();

    const saveEffect = pipe(
      Effect.tryPromise(() =>
        fetch('/api/providers/openai', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ api_key: state.apiKey }),
        }),
      ),
      Effect.flatMap((resp) => {
        if (!resp.ok) {
          return Effect.fail(new Error('Failed to save provider key'));
        }
        return Effect.tryPromise(() => resp.json());
      }),
    );

    Effect.runPromise(saveEffect)
      .then(() => {
        state.loading = false;
        state.saved = true;
        render();
      })
      .catch((err) => {
        state.loading = false;
        state.error = err?.message || 'Unknown error';
        render();
      });
  });
}

render();
"#;

const ONBOARD_APP_CSS: &str = r#"
:root {
  color-scheme: dark;
  --bg: #020617;
  --panel: #0f172a;
  --panel-border: #1e293b;
  --text: #f1f5f9;
  --muted: #cbd5e1;
  --muted-2: #94a3b8;
  --accent: #10b981;
  --danger: #f87171;
}

* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }
body.onboard-body {
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif;
}
.onboard-main { max-width: 768px; margin: 0 auto; padding: 48px 24px; }
.onboard-title { margin: 0; font-size: 32px; font-weight: 650; letter-spacing: -0.02em; }
.onboard-subtitle { margin: 10px 0 0; color: var(--muted); }
.onboard-app { margin-top: 28px; }
.card {
  border: 1px solid var(--panel-border);
  border-radius: 14px;
  background: rgba(15, 23, 42, 0.75);
  padding: 24px;
}
.step { font-size: 12px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted-2); margin: 0; }
.card-title { margin: 10px 0 0; font-size: 24px; font-weight: 550; }
.card-note { margin: 10px 0 0; color: var(--muted); font-size: 14px; }
.btn-provider {
  margin-top: 18px;
  width: 100%;
  text-align: left;
  border-radius: 10px;
  border: 1px solid rgba(16,185,129,.45);
  background: rgba(16,185,129,.1);
  color: var(--text);
  padding: 12px 14px;
}
.btn-provider-title { display: block; font-size: 14px; font-weight: 600; }
.btn-provider-note { display: block; margin-top: 5px; font-size: 12px; color: var(--muted); }
.field-label { margin-top: 20px; display: block; font-size: 14px; }
.field-input {
  margin-top: 8px;
  width: 100%;
  border-radius: 10px;
  border: 1px solid #334155;
  background: #020617;
  color: var(--text);
  padding: 10px 12px;
}
.actions { margin-top: 20px; display: flex; gap: 10px; }
.btn-secondary, .btn-primary {
  border-radius: 10px;
  padding: 8px 14px;
  font-size: 14px;
}
.btn-secondary {
  border: 1px solid #334155;
  background: transparent;
  color: var(--text);
}
.btn-primary {
  border: 1px solid var(--accent);
  background: var(--accent);
  color: #052e16;
  font-weight: 650;
}
.btn-primary:disabled { opacity: .5; }
.notice-error { margin: 12px 0 0; color: var(--danger); font-size: 14px; }
.notice-success { margin: 12px 0 0; color: var(--accent); font-size: 14px; }
code { color: #86efac; background: rgba(16,185,129,.08); padding: 2px 6px; border-radius: 6px; }
"#;
