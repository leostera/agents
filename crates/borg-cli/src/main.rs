use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::Duration,
};

use anyhow::Result;

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8080";
const DEFAULT_ONBOARD_PORT: u16 = 3777;
const DEFAULT_SCHEDULER_POLL_MS: u64 = 500;
const HEALTH_STATUS_OK: &str = "ok";
use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use borg_db::BorgDb;
use borg_exec::{ExecEngine, InboxMessage};
use borg_ltm::MemoryStore;
use borg_onboard::OnboardServer;
use borg_rt::RuntimeEngine;
use borg_ui::render_dashboard;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};
use turso::Builder;

#[derive(Parser, Debug)]
#[command(name = "borg", about = "Borg prototype runtime")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init {
        #[arg(long, default_value_t = DEFAULT_ONBOARD_PORT)]
        onboard_port: u16,
    },
    Start {
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        bind: String,
        #[arg(long, default_value_t = DEFAULT_SCHEDULER_POLL_MS)]
        poll_ms: u64,
    },
}

#[derive(Clone)]
struct AppState {
    db: BorgDb,
    exec: ExecEngine,
    memory: MemoryStore,
}

#[derive(Debug, Clone)]
struct BorgPaths {
    home: PathBuf,
    config_db: PathBuf,
    ltm_db: PathBuf,
}

impl BorgPaths {
    fn discover() -> Self {
        let home =
            Path::new(&std::env::var("HOME").unwrap_or_else(|_| ".".to_string())).join(".borg");
        Self {
            config_db: home.join("config.db"),
            ltm_db: home.join("ltm.db"),
            home,
        }
    }

    fn ensure_layout(&self) -> Result<()> {
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(self.home.join("logs"))?;
        Ok(())
    }
}

#[derive(Clone)]
struct BorgCliApp {
    paths: BorgPaths,
}

impl BorgCliApp {
    fn new(paths: BorgPaths) -> Self {
        Self { paths }
    }

    async fn init(&self, onboard_port: u16) -> Result<()> {
        info!(target: "borg_cli", onboard_port, "initializing borg runtime");
        self.initialize_storage().await?;

        let url = format!("http://localhost:{}/onboard", onboard_port);
        self.open_browser(&url);

        info!(target: "borg_cli", url, "borg init completed, onboarding server starting");
        println!("borg initialized. onboarding: {}", url);

        OnboardServer::new(onboard_port).run().await
    }

    async fn start(&self, bind: String, poll_ms: u64) -> Result<()> {
        info!(target: "borg_cli", config_db = %self.paths.config_db.display(), bind, poll_ms, "starting borg machine");

        self.paths.ensure_layout()?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(&self.paths.ltm_db)?;
        let exec = ExecEngine::new(
            db.clone(),
            memory.clone(),
            RuntimeEngine,
            format!("worker-{}", std::process::id()),
        );

        db.migrate().await?;
        memory.migrate().await?;

        let app_state = AppState {
            db: db.clone(),
            exec: exec.clone(),
            memory,
        };

        let scheduler = tokio::spawn(async move {
            info!(target: "borg_cli", "scheduler loop started");
            loop {
                match exec.run_once().await {
                    Ok(true) => debug!(target: "borg_cli", "scheduler processed one task"),
                    Ok(false) => debug!(target: "borg_cli", "scheduler tick had no work"),
                    Err(err) => error!(target: "borg_cli", error = %err, "scheduler tick failed"),
                }
                tokio::time::sleep(Duration::from_millis(poll_ms)).await;
            }
        });

        let router = Router::new()
            .route("/", get(ui_dashboard))
            .route("/health", get(health))
            .route("/ports/http/inbox", post(http_inbox))
            .route("/tasks", get(list_tasks))
            .route("/tasks/:id", get(get_task))
            .route("/tasks/:id/events", get(get_task_events))
            .route("/memory/search", get(memory_search))
            .route("/memory/entities/:id", get(get_memory_entity))
            .with_state(app_state);

        let addr: SocketAddr = bind.parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!(target: "borg_cli", address = %addr, "http server listening");

        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed waiting for ctrl-c signal");
            info!(target: "borg_cli", "received ctrl-c, shutting down");
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await?;

        scheduler.abort();
        Ok(())
    }

    async fn initialize_storage(&self) -> Result<()> {
        self.paths.ensure_layout()?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(&self.paths.ltm_db)?;

        db.migrate().await?;
        memory.migrate().await?;
        Ok(())
    }

    async fn open_config_db(&self) -> Result<BorgDb> {
        let config_path = self.paths.config_db.to_string_lossy().to_string();
        let db_handle = Builder::new_local(&config_path).build().await?;
        let conn = db_handle.connect()?;
        Ok(BorgDb::new(conn))
    }

    fn open_browser(&self, url: &str) {
        let mut commands: Vec<ProcessCommand> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            let mut cmd = ProcessCommand::new("open");
            cmd.arg(url);
            commands.push(cmd);
        }

        #[cfg(target_os = "linux")]
        {
            let mut cmd = ProcessCommand::new("xdg-open");
            cmd.arg(url);
            commands.push(cmd);
        }

        #[cfg(target_os = "windows")]
        {
            let mut cmd = ProcessCommand::new("cmd");
            cmd.arg("/C").arg("start").arg(url);
            commands.push(cmd);
        }

        let opened = commands.into_iter().any(|mut c| c.spawn().is_ok());
        if opened {
            info!(target: "borg_cli", url, "opened onboarding url in browser");
        } else {
            warn!(target: "borg_cli", url, "failed to auto-open browser; open url manually");
        }
    }
}

#[derive(Deserialize)]
struct TasksQuery {
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct MemorySearchQuery {
    q: String,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "info,borg_cli=debug,borg_db=debug,borg_exec=debug,borg_ltm=debug,borg_rt=debug,borg_onboard=debug"
                .to_string()
        }))
        .init();

    let app = BorgCliApp::new(BorgPaths::discover());
    match Cli::parse().cmd {
        Command::Init { onboard_port } => app.init(onboard_port).await,
        Command::Start { bind, poll_ms } => app.start(bind, poll_ms).await,
    }
}

async fn health() -> impl IntoResponse {
    debug!(target: "borg_cli", "health endpoint called");
    Json(json!({ "status": HEALTH_STATUS_OK }))
}

async fn ui_dashboard(State(state): State<AppState>) -> impl IntoResponse {
    debug!(target: "borg_cli", "ui dashboard endpoint called");
    let tasks_count = state
        .db
        .list_tasks(None, 10_000)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let entities_count = state
        .memory
        .search("movie", None, 10_000)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    Html(render_dashboard(tasks_count, entities_count))
}

async fn http_inbox(
    State(state): State<AppState>,
    Json(payload): Json<InboxMessage>,
) -> impl IntoResponse {
    info!(target: "borg_cli", user_key = payload.user_key, text = payload.text, "received HTTP inbox event");
    match state.exec.enqueue_user_message(payload).await {
        Ok(task_id) => (StatusCode::OK, Json(json!({ "task_id": task_id }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    debug!(target: "borg_cli", status = ?query.status, limit, "listing tasks endpoint");
    match state.db.list_tasks(query.status, limit).await {
        Ok(tasks) => (StatusCode::OK, Json(json!({ "tasks": tasks }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_task(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_cli", task_id, "get task endpoint");
    match state.db.get_task(&task_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "task not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_task_events(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_cli", task_id, "get task events endpoint");
    match state.db.get_task_events(&task_id).await {
        Ok(events) => (StatusCode::OK, Json(json!({ "events": events }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn memory_search(
    State(state): State<AppState>,
    Query(query): Query<MemorySearchQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(25);
    debug!(target: "borg_cli", q = query.q, entity_type = ?query.entity_type, limit, "memory search endpoint");

    match state
        .memory
        .search(&query.q, query.entity_type.as_deref(), limit)
        .await
    {
        Ok(entities) => (StatusCode::OK, Json(json!({ "entities": entities }))).into_response(),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn get_memory_entity(
    State(state): State<AppState>,
    AxumPath(entity_id): AxumPath<String>,
) -> impl IntoResponse {
    debug!(target: "borg_cli", entity_id, "get memory entity endpoint");
    match state.memory.get_entity(&entity_id).await {
        Ok(Some(entity)) => (StatusCode::OK, Json(json!({ "entity": entity }))).into_response(),
        Ok(None) => api_error(StatusCode::NOT_FOUND, "entity not found".to_string()),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

fn api_error(status: StatusCode, error: String) -> axum::response::Response {
    (status, Json(ApiError { error })).into_response()
}
