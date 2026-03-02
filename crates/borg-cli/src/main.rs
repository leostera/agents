use std::{collections::BTreeMap, io, io::Write};

use anyhow::Result;
use borg_api::BorgApiServer;
use borg_apps::DefaultAppsCatalog;
use borg_codemode::CodeModeRuntime;
use borg_core::{Uri, borgdir::BorgDir};
use borg_db::BorgDb;
use borg_exec::{BorgInput, BorgMessage, BorgRuntime, BorgSupervisor, JsonPortContext};
use borg_memory::{FactInput, MemoryStore, SearchQuery};
use borg_shellmode::ShellModeRuntime;
use borg_taskgraph::{TaskDispatch, TaskGraphSupervisor};
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::fs;
use tracing::info;

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
    long_about = "Manage Borg services, sessions, memory, and configuration."
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
    /// Runtime administration commands.
    Admin {
        /// Admin subcommand.
        #[command(subcommand)]
        cmd: AdminCommand,
    },
    /// Invoke subsystem tools directly.
    Tools {
        /// Tool namespace (for example: memory, codemode, shell, taskgraph, list).
        namespace: String,
        /// Command and optional inline JSON payload.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        /// Optional JSON payload (overrides inline payload when provided).
        #[arg(long)]
        payload: Option<String>,
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

#[derive(Subcommand, Debug)]
enum AdminCommand {
    /// TaskGraph maintenance commands.
    Tasks {
        /// Tasks subcommand.
        #[command(subcommand)]
        cmd: AdminTasksCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AdminTasksCommand {
    /// Delete all TaskGraph tasks (and cascaded taskgraph rows).
    ClearAllTasks {
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
        let app_seed_summary = DefaultAppsCatalog::new().install_missing(&db).await?;
        info!(
            target: "borg_cli",
            apps_created = app_seed_summary.apps_created,
            capabilities_created = app_seed_summary.capabilities_created,
            "default apps reconciled"
        );
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
        let shell_runtime = ShellModeRuntime::new();
        let runtime = BorgRuntime::new(db.clone(), memory.clone(), runtime, shell_runtime);
        let runtime = std::sync::Arc::new(runtime);
        let supervisor = BorgSupervisor::new(runtime.clone());
        let (task_dispatch_tx, mut task_dispatch_rx) =
            tokio::sync::mpsc::channel::<TaskDispatch>(128);
        let supervisor_for_tasks = supervisor.clone();
        tokio::spawn(async move {
            while let Some(task) = task_dispatch_rx.recv().await {
                let Ok(session_id) = Uri::parse(&task.assignee_session_uri) else {
                    tracing::warn!(
                        target: "borg_cli",
                        session_uri = %task.assignee_session_uri,
                        task_uri = %task.task_uri,
                        "invalid assignee session uri for task dispatch"
                    );
                    continue;
                };
                let user_id = Uri::from_parts("borg", "user", Some("taskgraph"))
                    .expect("valid synthetic taskgraph user uri");
                let payload = json!({
                    "port": "taskgraph",
                    "task_uri": task.task_uri,
                    "assignee_agent_id": task.assignee_agent_id,
                });
                let text = format!(
                    "This is your task: {}\n\
                     Task URI: {}\n\
                     Description: {}\n\
                     Definition of done: {}\n\n\
                     Execution policy:\n\
                     1. Execute this task yourself in the least number of steps possible.\n\
                     2. Do not create new tasks by default.\n\
                     3. Only create subtasks if there are more than 10 independent steps that can be parallelized for a clear speedup.\n\
                     4. If subtasks are not justified by rule #3, continue doing the work directly and finish it.",
                    task.title, task.task_uri, task.description, task.definition_of_done
                );
                supervisor_for_tasks
                    .cast(BorgMessage {
                        user_id,
                        session_id,
                        input: BorgInput::Chat { text },
                        port_context: std::sync::Arc::new(JsonPortContext::new(payload)),
                    })
                    .await;
            }
        });
        let taskgraph_supervisor =
            TaskGraphSupervisor::new(db.clone()).with_dispatch(task_dispatch_tx);
        taskgraph_supervisor.start().await;
        info!(target: "borg_cli", "taskgraph supervisor started");

        let api_server = BorgApiServer::new(bind, runtime, supervisor);
        api_server.run().await
    }

    async fn initialize_storage(&self) -> Result<()> {
        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;

        db.migrate().await?;
        let app_seed_summary = DefaultAppsCatalog::new().install_missing(&db).await?;
        info!(
            target: "borg_cli",
            apps_created = app_seed_summary.apps_created,
            capabilities_created = app_seed_summary.capabilities_created,
            "default apps reconciled"
        );
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

    async fn admin_tasks_clear_all(&self, yes: bool) -> Result<()> {
        if !yes && !confirm_taskgraph_clear_all()? {
            println!("aborted");
            return Ok(());
        }

        let db = self.open_config_db().await?;
        db.migrate().await?;
        let store = borg_taskgraph::TaskGraphStore::new(db);
        let deleted = store.clear_all_tasks().await?;
        info!(
            target: "borg_cli",
            deleted_tasks = deleted,
            "cleared all taskgraph tasks"
        );
        println!("cleared {} task(s) from taskgraph", deleted);
        Ok(())
    }

    async fn tools(
        &self,
        namespace: String,
        args: Vec<String>,
        payload: Option<String>,
    ) -> Result<()> {
        let namespace = namespace.trim().to_ascii_lowercase();
        if namespace == "list" {
            println!("{}", serde_json::to_string(&build_tools_catalog())?);
            return Ok(());
        }

        if args.is_empty() {
            anyhow::bail!("missing tools command; use `borg tools list`");
        }
        if args.len() == 1 && args[0] == "list" {
            let commands = namespace_commands(&namespace)?;
            let output = json!({
                "ok": true,
                "namespace": namespace,
                "commands": commands,
            });
            println!("{}", serde_json::to_string(&output)?);
            return Ok(());
        }

        let known_commands = namespace_commands(&namespace)?;
        let (command, payload_value) = resolve_tools_command_and_payload(
            &namespace,
            &known_commands,
            &args,
            payload.as_deref(),
        )?;

        let result = match namespace.as_str() {
            "codemode" => borg_codemode::cli::run(&command, payload_value).await?,
            "shell" => borg_shellmode::cli::run(&command, payload_value).await?,
            "memory" => {
                let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;
                memory.migrate().await?;
                borg_memory::cli::run(memory, &command, payload_value).await?
            }
            "taskgraph" => {
                let db = self.open_config_db().await?;
                db.migrate().await?;
                borg_taskgraph::cli::run(db, &command, payload_value).await?
            }
            other => anyhow::bail!("unknown tools namespace `{}`; use `borg tools list`", other),
        };

        println!("{}", serde_json::to_string(&result)?);
        Ok(())
    }
}

fn build_tools_catalog() -> Value {
    let mut namespaces = BTreeMap::new();
    namespaces.insert("codemode", borg_codemode::cli::command_names());
    namespaces.insert("memory", borg_memory::cli::command_names());
    namespaces.insert("shell", borg_shellmode::cli::command_names());
    namespaces.insert("taskgraph", borg_taskgraph::cli::command_names());
    json!({
        "ok": true,
        "namespaces": namespaces,
    })
}

fn namespace_commands(namespace: &str) -> Result<Vec<&'static str>> {
    let commands = match namespace {
        "codemode" => borg_codemode::cli::command_names(),
        "memory" => borg_memory::cli::command_names(),
        "shell" => borg_shellmode::cli::command_names(),
        "taskgraph" => borg_taskgraph::cli::command_names(),
        other => anyhow::bail!("unknown tools namespace `{}`; use `borg tools list`", other),
    };
    Ok(commands)
}

fn resolve_tools_command_and_payload(
    namespace: &str,
    known_commands: &[&str],
    args: &[String],
    payload_flag: Option<&str>,
) -> Result<(String, Value)> {
    if args.is_empty() {
        anyhow::bail!("missing {} command", namespace);
    }

    if let Some(payload_text) = payload_flag {
        let command = args.join(" ");
        if !known_commands.iter().any(|candidate| *candidate == command) {
            anyhow::bail!(
                "unknown {} command `{}`; use `borg tools {} list`",
                namespace,
                command,
                namespace
            );
        }
        let payload = parse_payload_json(payload_text)?;
        return Ok((command, payload));
    }

    for index in (1..=args.len()).rev() {
        let candidate = args[..index].join(" ");
        if !known_commands.iter().any(|known| *known == candidate) {
            continue;
        }

        if index == args.len() {
            return Ok((candidate, json!({})));
        }

        let payload_text = args[index..].join(" ");
        let payload_value = parse_payload_json(&payload_text)?;
        return Ok((candidate, payload_value));
    }

    anyhow::bail!(
        "unknown {} command `{}`; use `borg tools {} list`",
        namespace,
        args.join(" "),
        namespace
    )
}

fn parse_payload_json(payload_text: &str) -> Result<Value> {
    serde_json::from_str::<Value>(payload_text)
        .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, payload_text))
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

fn confirm_taskgraph_clear_all() -> Result<bool> {
    print!("This will permanently delete all TaskGraph tasks. Continue? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "info,borg_cli=debug,borg_api=debug,borg_ports=debug,borg_db=debug,borg_exec=debug,borg_memory=debug,borg_codemode=debug"
                .to_string()
        }))
        .init();

    let borg_dir = BorgDir::new();
    borg_dir.ensure_initialized().await?;
    let app = BorgCliApp::new(borg_dir);
    match Cli::parse().cmd {
        Command::Init { onboard_port } => app.init(onboard_port).await,
        Command::Start { bind } => app.start(bind).await,
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
        Command::Admin { cmd } => match cmd {
            AdminCommand::Tasks { cmd } => match cmd {
                AdminTasksCommand::ClearAllTasks { yes } => app.admin_tasks_clear_all(yes).await,
            },
        },
        Command::Tools {
            namespace,
            args,
            payload,
        } => {
            if let Err(err) = app.tools(namespace, args, payload).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
                return Ok(());
            }
            Ok(())
        }
    }
}
