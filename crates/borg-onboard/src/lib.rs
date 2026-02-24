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
    <script src=\"https://cdn.tailwindcss.com\"></script>
  </head>
  <body class=\"min-h-screen bg-slate-950 text-slate-100\">
    <main class=\"mx-auto max-w-3xl px-6 py-12\">
      <h1 class=\"text-3xl font-semibold tracking-tight\">Borg Onboarding</h1>
      <p class=\"mt-2 text-slate-300\">Configure your first provider to start Borg.</p>
      <div id=\"app\" class=\"mt-8\"></div>
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
      <section class=\"rounded-xl border border-slate-800 bg-slate-900/70 p-6\">
        <p class=\"text-xs uppercase tracking-wide text-slate-400\">Step 1 of 2</p>
        <h2 class=\"mt-2 text-xl font-medium\">Choose LLM Provider</h2>
        <button id=\"choose-openai\" class=\"mt-6 w-full rounded-lg border border-emerald-400/50 bg-emerald-500/10 px-4 py-3 text-left\">
          <span class=\"block text-sm font-medium\">OpenAI (API key)</span>
          <span class=\"block text-xs text-slate-300 mt-1\">Currently the only provider supported in onboarding.</span>
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
    <section class=\"rounded-xl border border-slate-800 bg-slate-900/70 p-6\">
      <p class=\"text-xs uppercase tracking-wide text-slate-400\">Step 2 of 2</p>
      <h2 class=\"mt-2 text-xl font-medium\">Enter OpenAI API Key</h2>
      <p class=\"mt-2 text-sm text-slate-300\">This will be stored in <code>~/.borg/config.db</code> under <code>providers</code>.</p>
      <label class=\"mt-6 block text-sm\">API Key</label>
      <input id=\"api-key\" type=\"password\" placeholder=\"sk-...\" class=\"mt-2 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 outline-none focus:border-emerald-400\" value=\"${state.apiKey}\" />
      ${state.error ? `<p class=\"mt-3 text-sm text-red-400\">${state.error}</p>` : ''}
      ${state.saved ? `<p class=\"mt-3 text-sm text-emerald-400\">Saved. You can now run <code>borg start</code>.</p>` : ''}
      <div class=\"mt-6 flex gap-3\">
        <button id=\"back\" class=\"rounded-lg border border-slate-700 px-4 py-2 text-sm\">Back</button>
        <button id=\"save\" class=\"rounded-lg bg-emerald-500 px-4 py-2 text-sm font-medium text-black disabled:opacity-50\" ${state.loading ? 'disabled' : ''}>
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
