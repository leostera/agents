use std::{io, io::Write};

use anyhow::Result;
use borg_api::BorgApiServer;
use borg_apps::DefaultAppsCatalog;
use borg_codemode::CodeModeRuntime;
use borg_core::{Uri, borgdir::BorgDir};
use borg_db::BorgDb;
use borg_exec::{BorgInput, BorgMessage, BorgRuntime, BorgSupervisor, JsonPortContext};
use borg_fs::BorgFs;
use borg_memory::{FactInput, MemoryStore, SearchQuery};
use borg_shellmode::ShellModeRuntime;
use borg_taskgraph::{TaskDispatch, TaskGraphStore, TaskGraphSupervisor};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::fs;
use tokio::time::{Duration, interval};
use tracing::info;

pub(crate) const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8080";
pub(crate) const DEFAULT_ONBOARD_PORT: u16 = 3777;
pub(crate) const DEFAULT_POLL_INTERVAL_MS: u64 = 500;
const OPENAI_PROVIDER: &str = "openai";
const OPENROUTER_PROVIDER: &str = "openrouter";
const RUNTIME_SETTINGS_PORT: &str = "runtime";
const RUNTIME_PREFERRED_PROVIDER_KEY: &str = "preferred_provider";
const RUNTIME_PREFERRED_PROVIDER_ID_KEY: &str = "preferred_provider_id";

#[derive(Clone)]
pub(crate) struct BorgCliApp {
    borg_dir: BorgDir,
}

impl BorgCliApp {
    pub(crate) fn new(borg_dir: BorgDir) -> Self {
        Self { borg_dir }
    }

    pub(crate) async fn init(&self, onboard_port: u16) -> Result<()> {
        info!(target: "borg_cli", onboard_port, "initializing borg runtime");
        self.initialize_storage().await?;
        Ok(())
    }

    pub(crate) async fn start(&self, bind: String) -> Result<()> {
        info!(target: "borg_cli", config_db = %self.borg_dir.config_db().display(), bind, "starting borg machine");

        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.memory_db(), self.borg_dir.memory_db())?;
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
        let files = BorgFs::local(db.clone(), self.borg_dir.files().to_path_buf());
        let runtime = BorgRuntime::new(db.clone(), memory.clone(), runtime, shell_runtime, files);
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
                if let Err(err) = supervisor_for_tasks
                    .cast(BorgMessage {
                        actor_id: session_id.clone(),
                        user_id,
                        session_id,
                        input: BorgInput::Chat { text },
                        port_context: std::sync::Arc::new(JsonPortContext::new(payload)),
                    })
                    .await
                {
                    tracing::warn!(
                        target: "borg_cli",
                        error = %err,
                        task_uri = %task.task_uri,
                        "failed to enqueue task dispatch actor message"
                    );
                }
            }
        });
        let taskgraph_supervisor =
            TaskGraphSupervisor::new(db.clone()).with_dispatch(task_dispatch_tx);
        taskgraph_supervisor.start().await;
        info!(target: "borg_cli", "taskgraph supervisor started");

        self.start_clockwork_supervisor(db.clone());
        info!(target: "borg_cli", "clockwork supervisor started");

        let api_server = BorgApiServer::new(bind, runtime, supervisor);
        api_server.run().await
    }

    fn start_clockwork_supervisor(&self, db: BorgDb) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;
                let now = chrono::Utc::now().to_rfc3339();
                match db.list_due_clockwork_jobs(&now, 100).await {
                    Ok(due_jobs) => {
                        if !due_jobs.is_empty() {
                            tracing::debug!(
                                target: "borg_cli",
                                due_count = due_jobs.len(),
                                "clockwork scaffold loop found due jobs"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            target: "borg_cli",
                            error = %error,
                            "clockwork supervisor poll failed"
                        );
                    }
                }
            }
        });
    }

    async fn initialize_storage(&self) -> Result<()> {
        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.memory_db(), self.borg_dir.memory_db())?;

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

    pub(crate) async fn open_config_db(&self) -> Result<BorgDb> {
        self.borg_dir.ensure_initialized().await?;
        let config_path = self.borg_dir.config_db().to_string_lossy().to_string();
        BorgDb::open_local(&config_path).await
    }

    pub(crate) async fn open_memory_store(&self) -> Result<MemoryStore> {
        self.borg_dir.ensure_initialized().await?;
        let memory = MemoryStore::new(self.borg_dir.memory_db(), self.borg_dir.memory_db())?;
        memory.migrate().await?;
        Ok(memory)
    }

    pub(crate) async fn open_borg_fs(&self) -> Result<BorgFs> {
        self.borg_dir.ensure_initialized().await?;
        let db = self.open_config_db().await?;
        db.migrate().await?;
        Ok(BorgFs::local(db, self.borg_dir.files().to_path_buf()))
    }

    pub(crate) async fn config_set(&self, key: String, value: String) -> Result<()> {
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
                if provider.is_empty() {
                    anyhow::bail!("providers.default must not be empty");
                }
                db.upsert_port_setting(
                    RUNTIME_SETTINGS_PORT,
                    RUNTIME_PREFERRED_PROVIDER_KEY,
                    provider.as_str(),
                )
                .await?;
                db.upsert_port_setting(
                    RUNTIME_SETTINGS_PORT,
                    RUNTIME_PREFERRED_PROVIDER_ID_KEY,
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

    pub(crate) async fn session(&self, session_id: String, poll_ms: u64) -> Result<()> {
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
                    let messages = db.list_session_messages(&session_id, next_index, 512).await?;
                    for message in messages {
                        println!("{}", serde_json::to_string(&message)?);
                        next_index += 1;
                    }
                }
            }
        }
    }

    pub(crate) async fn session_clear_history(&self, session_id: String) -> Result<()> {
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

    pub(crate) async fn memory_clear(&self, yes: bool) -> Result<()> {
        if !yes && !confirm_memory_clear()? {
            println!("aborted");
            return Ok(());
        }
        remove_file_if_exists(self.borg_dir.memory_db()).await?;
        let memory = MemoryStore::new(self.borg_dir.memory_db(), self.borg_dir.memory_db())?;
        memory.migrate().await?;
        info!(
            target: "borg_cli",
            memory_db = %self.borg_dir.memory_db().display(),
            "cleared and reinitialized memory database"
        );
        println!("cleared and reinitialized memory database");
        Ok(())
    }

    pub(crate) async fn admin_tasks_clear_all(&self, yes: bool) -> Result<()> {
        if !yes && !confirm_taskgraph_clear_all()? {
            println!("aborted");
            return Ok(());
        }

        let db = self.open_config_db().await?;
        db.migrate().await?;
        let store = TaskGraphStore::new(db);
        let deleted = store.clear_all_tasks().await?;
        info!(
            target: "borg_cli",
            deleted_tasks = deleted,
            "cleared all taskgraph tasks"
        );
        println!("cleared {} task(s) from taskgraph", deleted);
        Ok(())
    }

    pub(crate) async fn admin_sessions_clear_all(&self, all: bool, yes: bool) -> Result<()> {
        if !all {
            anyhow::bail!("refusing to clear sessions without --all");
        }
        if !yes && !confirm_sessions_clear_all()? {
            println!("aborted");
            return Ok(());
        }

        let db = self.open_config_db().await?;
        db.migrate().await?;

        let mut deleted_sessions = 0_u64;
        let mut deleted_messages = 0_u64;
        loop {
            let sessions = db.list_sessions(500, None, None).await?;
            if sessions.is_empty() {
                break;
            }
            for session in sessions {
                deleted_messages += db.clear_session_history(&session.session_id).await?;
                deleted_sessions += db.delete_session(&session.session_id).await?;
            }
        }

        info!(
            target: "borg_cli",
            deleted_sessions,
            deleted_messages,
            "cleared all sessions and session history"
        );
        println!(
            "cleared {} session(s) and {} message(s)",
            deleted_sessions, deleted_messages
        );
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

async fn remove_file_if_exists(path: &std::path::Path) -> Result<()> {
    match fs::remove_file(path).await {
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

fn confirm_sessions_clear_all() -> Result<bool> {
    print!("This will permanently delete all sessions and session messages. Continue? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
}
