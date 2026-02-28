use std::{io, io::Write};

use anyhow::Result;
use borg_api::BorgApiServer;
use borg_core::{Uri, borgdir::BorgDir};
use borg_db::BorgDb;
use borg_exec::ExecEngine;
use borg_ltm::{FactInput, MemoryStore, SearchQuery};
use borg_rt::CodeModeRuntime;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::fs;
use tracing::{error, info};
use uuid::Uuid;

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8080";
const DEFAULT_ONBOARD_PORT: u16 = 3777;
const DEFAULT_POLL_INTERVAL_MS: u64 = 500;
const OPENAI_PROVIDER: &str = "openai";
const OPENROUTER_PROVIDER: &str = "openrouter";
const RUNTIME_SETTINGS_PORT: &str = "runtime";
const RUNTIME_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";

#[derive(Parser, Debug)]
#[command(
    name = "borg",
    about = "Borg runtime CLI",
    long_about = "Manage Borg services, tasks, sessions, memory, and configuration."
)]
struct Cli {
    /// Top-level command to run.
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize local Borg storage and start onboarding server.
    Init {
        /// Port for the onboarding web server.
        #[arg(long, default_value_t = DEFAULT_ONBOARD_PORT)]
        onboard_port: u16,
    },
    /// Start the Borg runtime (API + executor loop).
    Start {
        /// API bind address in host:port form.
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        bind: String,
    },
    /// Task operations against the API.
    Task {
        /// Task subcommand.
        #[command(subcommand)]
        cmd: TaskCommand,
        /// API address in host:port form.
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        api: String,
        /// Poll interval (ms) for streaming task events.
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS)]
        poll_ms: u64,
    },
    /// Stream task events by task id.
    Events {
        /// Task URI (for example borg:task:...).
        task_id: String,
        /// API address in host:port form.
        #[arg(long, default_value = DEFAULT_HTTP_BIND)]
        api: String,
        /// Poll interval (ms) for checking new events.
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS)]
        poll_ms: u64,
    },
    /// Session utilities (stream and history operations).
    Session {
        /// Session subcommand.
        #[command(subcommand)]
        cmd: SessionCommand,
    },
    /// Memory maintenance commands.
    Memory {
        /// Memory subcommand.
        #[command(subcommand)]
        cmd: MemoryCommand,
    },
    /// Set persisted runtime configuration values.
    Config {
        /// Config subcommand.
        #[command(subcommand)]
        cmd: ConfigCommand,
    },
}

#[derive(Subcommand, Debug)]
enum TaskCommand {
    /// Fetch one task by id.
    Get {
        /// Task URI (for example borg:task:...).
        id: String,
    },
    /// Send a new user message to the HTTP port and stream events.
    New {
        /// User message text.
        text: String,
        /// User URI (for example borg:user:alice).
        #[arg(long)]
        user_key: Option<String>,
        /// Optional existing session URI.
        #[arg(long)]
        session_id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    /// Persist one config key/value pair.
    Set {
        /// Config key (for example providers.openai, providers.default, or ports.telegram).
        key: String,
        /// Config value to persist.
        value: String,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCommand {
    /// Stream messages for a session.
    Stream {
        /// Session URI (for example borg:session:...).
        session_id: String,
        /// Poll interval (ms) for checking new session messages.
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS)]
        poll_ms: u64,
    },
    /// Delete all persisted messages for a session.
    ClearHistory {
        /// Session URI to clear.
        session_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum MemoryCommand {
    /// Clear all LTM fact store and derived search/graph data.
    Clear {
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
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
        Ok(())
    }

    async fn start(&self, bind: String) -> Result<()> {
        info!(target: "borg_cli", config_db = %self.borg_dir.config_db().display(), bind, "starting borg machine");

        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;
        db.migrate().await?;
        memory.migrate().await?;

        let memory_for_state_facts = memory.clone();
        let memory_for_search = memory.clone();
        let runtime = CodeModeRuntime::default()
            .with_ffi_handler("memory__state_facts", move |args| {
                ffi_memory_state_facts(memory_for_state_facts.clone(), args)
            })
            .with_ffi_handler("memory__search", move |args| {
                ffi_memory_search(memory_for_search.clone(), args)
            });
        let exec = ExecEngine::new(
            db.clone(),
            memory.clone(),
            runtime,
            Uri::parse(&format!("borg:worker:{}", Uuid::now_v7()))?,
        );

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
        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;

        db.migrate().await?;
        memory.migrate().await?;
        Ok(())
    }

    async fn open_config_db(&self) -> Result<BorgDb> {
        self.borg_dir.ensure_initialized().await?;
        let config_path = self.borg_dir.config_db().to_string_lossy().to_string();
        BorgDb::open_local(&config_path).await
    }

    async fn config_set(&self, key: String, value: String) -> Result<()> {
        let db = self.open_config_db().await?;
        db.migrate().await?;

        match key.as_str() {
            "providers.openai" => {
                db.upsert_provider_api_key(OPENAI_PROVIDER, value.trim())
                    .await?;
                info!(target: "borg_cli", key, "config value updated");
                println!("ok");
                Ok(())
            }
            "providers.openrouter" => {
                db.upsert_provider_api_key(OPENROUTER_PROVIDER, value.trim())
                    .await?;
                info!(target: "borg_cli", key, "config value updated");
                println!("ok");
                Ok(())
            }
            "providers.default" => {
                let provider = value.trim().to_ascii_lowercase();
                if provider != OPENAI_PROVIDER && provider != OPENROUTER_PROVIDER {
                    anyhow::bail!(
                        "unsupported providers.default `{}` (expected `openai` or `openrouter`)",
                        provider
                    );
                }
                db.upsert_port_setting(
                    RUNTIME_SETTINGS_PORT,
                    RUNTIME_PREFERRED_PROVIDER_KEY,
                    provider.as_str(),
                )
                .await?;
                info!(target: "borg_cli", key, "config value updated");
                println!("ok");
                Ok(())
            }
            "ports.telegram" => {
                let existing = db.get_port("telegram").await?;
                let mut settings = existing
                    .as_ref()
                    .map(|port| port.settings.clone())
                    .unwrap_or_else(|| json!({}));
                if let Some(map) = settings.as_object_mut() {
                    map.insert(
                        "bot_token".to_string(),
                        Value::String(value.trim().to_string()),
                    );
                } else {
                    settings = json!({ "bot_token": value.trim() });
                }
                let enabled = existing.as_ref().map(|port| port.enabled).unwrap_or(true);
                let allows_guests = existing
                    .as_ref()
                    .map(|port| port.allows_guests)
                    .unwrap_or(true);
                let default_agent_id = existing
                    .as_ref()
                    .and_then(|port| port.default_agent_id.as_ref());
                db.upsert_port(
                    "telegram",
                    "telegram",
                    enabled,
                    allows_guests,
                    default_agent_id,
                    &settings,
                )
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

    async fn session(&self, session_id: String, poll_ms: u64) -> Result<()> {
        let session_id = Uri::parse(&session_id).map_err(|_| {
            anyhow::anyhow!(
                "invalid session id `{}` (expected URI like borg:session:<id>)",
                session_id
            )
        })?;
        let db = self.open_config_db().await?;

        let mut next_index: usize = 0;
        loop {
            tokio::select! {
                ctrl = tokio::signal::ctrl_c() => {
                    ctrl?;
                    info!(target: "borg_cli", session_id = %session_id, "session stream interrupted by ctrl-c");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(poll_ms)) => {
                    let messages = db
                        .list_session_messages(&session_id, next_index, 512)
                        .await?;
                    for message in messages {
                        println!("{}", serde_json::to_string(&message)?);
                        next_index += 1;
                    }
                }
            }
        }
    }

    async fn session_clear_history(&self, session_id: String) -> Result<()> {
        let session_id = Uri::parse(&session_id).map_err(|_| {
            anyhow::anyhow!(
                "invalid session id `{}` (expected URI like borg:session:<id>)",
                session_id
            )
        })?;
        let db = self.open_config_db().await?;
        db.migrate().await?;
        let deleted = db.clear_session_history(&session_id).await?;
        info!(
            target: "borg_cli",
            session_id = %session_id,
            deleted,
            "cleared session history"
        );
        println!("cleared {} message(s) for {}", deleted, session_id);
        Ok(())
    }

    async fn memory_clear(&self, yes: bool) -> Result<()> {
        if !yes && !confirm_memory_clear()? {
            println!("aborted");
            return Ok(());
        }
        remove_dir_if_exists(self.borg_dir.ltm_db()).await?;
        remove_dir_if_exists(self.borg_dir.search_db()).await?;
        fs::create_dir_all(self.borg_dir.ltm_db()).await?;
        fs::create_dir_all(self.borg_dir.search_db()).await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;
        memory.migrate().await?;
        info!(
            target: "borg_cli",
            ltm_db = %self.borg_dir.ltm_db().display(),
            search_db = %self.borg_dir.search_db().display(),
            "cleared and reinitialized memory databases"
        );
        println!("cleared and reinitialized memory stores");
        Ok(())
    }
}

fn parse_single_arg<T: DeserializeOwned>(args: Vec<Value>, op_name: &str) -> Result<T> {
    let Some(first) = args.into_iter().next() else {
        anyhow::bail!("{op_name} expects one argument");
    };
    serde_json::from_value(first)
        .map_err(|err| anyhow::anyhow!("{op_name} argument decode error: {err}"))
}

fn ffi_memory_state_facts(memory: MemoryStore, args: Vec<Value>) -> Result<Value> {
    let facts: Vec<FactInput> = parse_single_arg(args, "memory__state_facts")?;
    if facts.is_empty() {
        anyhow::bail!("memory__state_facts expects a non-empty facts array");
    }
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        let result = handle.block_on(memory.state_facts(facts))?;
        Ok(serde_json::to_value(result)?)
    })
}

fn ffi_memory_search(memory: MemoryStore, args: Vec<Value>) -> Result<Value> {
    let query: SearchQuery = parse_single_arg(args, "memory__search")?;
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        let result = handle.block_on(memory.search_query(query))?;
        Ok(serde_json::to_value(result)?)
    })
}

async fn remove_dir_if_exists(path: &std::path::Path) -> Result<()> {
    match fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn confirm_memory_clear() -> Result<bool> {
    print!(
        "This will permanently delete all Borg memory data (facts, graph, and search index). Continue? [y/N]: "
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
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
            "info,borg_cli=debug,borg_api=debug,borg_ports=debug,borg_db=debug,borg_exec=debug,borg_ltm=debug,borg_rt=debug"
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
        Command::Session { cmd } => match cmd {
            SessionCommand::Stream {
                session_id,
                poll_ms,
            } => app.session(session_id, poll_ms).await,
            SessionCommand::ClearHistory { session_id } => {
                app.session_clear_history(session_id).await
            }
        },
        Command::Memory { cmd } => match cmd {
            MemoryCommand::Clear { yes } => app.memory_clear(yes).await,
        },
        Command::Config { cmd } => match cmd {
            ConfigCommand::Set { key, value } => app.config_set(key, value).await,
        },
    }
}
