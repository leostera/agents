use std::process::Command as ProcessCommand;

use anyhow::Result;
use borg_api::BorgApiServer;
use borg_core::{Uri, borgdir::BorgDir};
use borg_db::BorgDb;
use borg_exec::ExecEngine;
use borg_ltm::MemoryStore;
use borg_onboard::OnboardServer;
use borg_rt::CodeModeRuntime;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tracing::{error, info, warn};
use uuid::Uuid;

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8080";
const DEFAULT_ONBOARD_PORT: u16 = 3777;
const DEFAULT_POLL_INTERVAL_MS: u64 = 500;

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
    },
    Task {
        #[command(subcommand)]
        cmd: TaskCommand,
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        api: String,
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS)]
        poll_ms: u64,
    },
    Events {
        task_id: String,
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        api: String,
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS)]
        poll_ms: u64,
    },
    Config {
        #[command(subcommand)]
        cmd: ConfigCommand,
    },
}

#[derive(Subcommand, Debug)]
enum TaskCommand {
    Get {
        id: String,
    },
    New {
        text: String,
        #[arg(long)]
        user_key: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Set { key: String, value: String },
}

#[derive(Clone)]
struct BorgCliApp {
    borg_dir: BorgDir,
}

impl BorgCliApp {
    fn new(borg_dir: BorgDir) -> Self {
        Self { borg_dir }
    }

    async fn init(&self, onboard_port: u16) -> Result<()> {
        info!(target: "borg_cli", onboard_port, "initializing borg runtime");
        self.initialize_storage().await?;

        let url = format!("http://localhost:{}/onboard", onboard_port);
        self.open_browser(&url);

        info!(target: "borg_cli", url, "borg init completed, onboarding server starting");
        println!("borg initialized. onboarding: {}", url);

        OnboardServer::new(onboard_port, self.borg_dir.config_db().to_path_buf())
            .run()
            .await
    }

    async fn start(&self, bind: String) -> Result<()> {
        info!(target: "borg_cli", config_db = %self.borg_dir.config_db().display(), bind, "starting borg machine");

        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;
        let exec = ExecEngine::new(
            db.clone(),
            CodeModeRuntime::default(),
            Uri::parse(&format!("borg:worker:{}", Uuid::now_v7()))?,
        );

        db.migrate().await?;
        memory.migrate().await?;

        let scheduler_exec = exec.clone();
        let scheduler = tokio::spawn(async move {
            info!(target: "borg_cli", "executor loop started");
            if let Err(err) = scheduler_exec.run().await {
                error!(target: "borg_cli", error = %err, "executor loop terminated");
            }
        });

        let api_server = BorgApiServer::new(bind, db, exec, memory);
        let result = api_server.run().await;
        scheduler.abort();
        result
    }

    async fn initialize_storage(&self) -> Result<()> {
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;

        db.migrate().await?;
        memory.migrate().await?;
        Ok(())
    }

    async fn open_config_db(&self) -> Result<BorgDb> {
        let config_path = self.borg_dir.config_db().to_string_lossy().to_string();
        BorgDb::open_local(&config_path).await
    }

    async fn config_set(&self, key: String, value: String) -> Result<()> {
        let db = self.open_config_db().await?;
        db.migrate().await?;

        match key.as_str() {
            "providers.openai" => {
                db.upsert_provider_api_key("openai", value.trim()).await?;
                info!(target: "borg_cli", key, "config value updated");
                println!("ok");
                Ok(())
            }
            "ports.telegram" => {
                db.upsert_port_setting("telegram", "bot_token", value.trim())
                    .await?;
                info!(target: "borg_cli", key, "config value updated");
                println!("ok");
                Ok(())
            }
            other => anyhow::bail!("unsupported config key `{}`", other),
        }
    }

    async fn task_get(&self, api: String, id: String) -> Result<()> {
        let client = Client::new();
        let url = format!("http://{}/tasks/{}", api, id);
        let response = client.get(&url).send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("request failed with {}: {}", status, body);
        }

        println!("{}", body);
        Ok(())
    }

    async fn task_new_and_stream(
        &self,
        api: String,
        text: String,
        user_key: Option<String>,
        session_id: Option<String>,
        poll_ms: u64,
    ) -> Result<()> {
        let resolved_user_key = user_key
            .as_ref()
            .filter(|key| !key.trim().is_empty())
            .cloned()
            .or_else(|| std::env::var("USERNAME").ok())
            .or_else(|| std::env::var("USER").ok())
            .filter(|key| !key.trim().is_empty())
            .unwrap_or_else(|| "cli".to_string());
        let user_key_uri = if resolved_user_key.contains(':') {
            Uri::parse(&resolved_user_key)?
        } else {
            let slug: String = resolved_user_key
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect();
            Uri::parse(&format!("borg:user:{}", slug))?
        };
        if let Some(explicit) = &user_key {
            Uri::parse(explicit).map_err(|_| {
                anyhow::anyhow!(
                    "invalid --user-key URI `{}` (expected URI like borg:user:alice)",
                    explicit
                )
            })?;
        }
        let parsed_session_id = match session_id {
            Some(raw) => Some(Uri::parse(&raw).map_err(|_| {
                anyhow::anyhow!(
                    "invalid --session-id URI `{}` (expected URI like borg:session:<id>)",
                    raw
                )
            })?),
            None => None,
        };

        let client = Client::new();
        let url = format!("http://{}/ports/http", api);
        let body = serde_json::json!({
            "user_key": user_key_uri,
            "text": text,
            "session_id": parsed_session_id,
            "metadata": {}
        });

        let response = client.post(&url).json(&body).send().await?;
        let status = response.status();
        let response_body = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("failed to create task with {}: {}", status, response_body);
        }

        let created: CreateTaskResponse = serde_json::from_str(&response_body)?;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "task_id": created.task_id,
                "session_id": created.session_id,
                "reply": created.reply,
            }))?
        );

        if let Some(task_id) = created.task_id {
            self.events(api, task_id.to_string(), poll_ms, true).await
        } else {
            Ok(())
        }
    }

    async fn events(
        &self,
        api: String,
        task_id: String,
        poll_ms: u64,
        stop_on_terminal: bool,
    ) -> Result<()> {
        let client = Client::new();
        let mut seen = std::collections::HashSet::<Uri>::new();
        let url = format!("http://{}/tasks/{}/events", api, task_id);

        loop {
            tokio::select! {
                ctrl = tokio::signal::ctrl_c() => {
                    ctrl?;
                    info!(target: "borg_cli", "events stream interrupted by ctrl-c");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(poll_ms)) => {
                    let response = client.get(&url).send().await?;
                    let status = response.status();
                    let body = response.text().await?;
                    if !status.is_success() {
                        anyhow::bail!("events request failed with {}: {}", status, body);
                    }

                    let parsed: TaskEventsResponse = serde_json::from_str(&body)?;
                    for event in parsed.events {
                        if seen.insert(event.event_id.clone()) {
                            println!("{}", serde_json::to_string(&event)?);
                            if stop_on_terminal
                                && (event.event_type.as_str() == "borg:task:succeeded"
                                    || event.event_type.as_str() == "borg:task:failed")
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
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

#[derive(Debug, Deserialize, serde::Serialize)]
struct TaskEventJson {
    event_id: Uri,
    task_id: Uri,
    ts: String,
    #[serde(rename = "event_type")]
    event_type: Uri,
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct TaskEventsResponse {
    events: Vec<TaskEventJson>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskResponse {
    task_id: Option<Uri>,
    session_id: Option<Uri>,
    reply: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "info,borg_cli=debug,borg_api=debug,borg_ports=debug,borg_db=debug,borg_exec=debug,borg_ltm=debug,borg_rt=debug,borg_onboard=debug"
                .to_string()
        }))
        .init();

    let borg_dir = BorgDir::new();
    borg_dir.ensure_initialized().await?;
    let app = BorgCliApp::new(borg_dir);
    match Cli::parse().cmd {
        Command::Init { onboard_port } => app.init(onboard_port).await,
        Command::Start { bind } => app.start(bind).await,
        Command::Task { cmd, api, poll_ms } => match cmd {
            TaskCommand::Get { id } => app.task_get(api, id).await,
            TaskCommand::New {
                text,
                user_key,
                session_id,
            } => {
                app.task_new_and_stream(api, text, user_key, session_id, poll_ms)
                    .await
            }
        },
        Command::Events {
            task_id,
            api,
            poll_ms,
        } => app.events(api, task_id, poll_ms, false).await,
        Command::Config { cmd } => match cmd {
            ConfigCommand::Set { key, value } => app.config_set(key, value).await,
        },
    }
}
