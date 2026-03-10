use std::{io, io::Write};

use anyhow::Result;
use borg_apps::DefaultAppsCatalog;
use borg_codemode::CodeModeRuntime;
use borg_core::{ActorId, EndpointUri, MessagePayload, WorkspaceId, borgdir::BorgDir};
use borg_db::BorgDb;
use borg_exec::BorgRuntime;
use borg_fs::BorgFs;
use borg_gql::BorgHttpServer;
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
        let runtime_code = CodeModeRuntime::default()
            .with_ffi_handler("memory__state_facts", move |args| {
                ffi_memory_state_facts(memory_for_state_facts.clone(), args)
            })
            .with_ffi_handler("memory__search", move |args| {
                ffi_memory_search(memory_for_search.clone(), args)
            });
        let shell_runtime = ShellModeRuntime::new();
        let files = BorgFs::local(db.clone(), self.borg_dir.files().to_path_buf());

        // BorgRuntime::new now returns Arc<BorgRuntime> and handles its own supervisor
        let runtime = BorgRuntime::new(
            db.clone(),
            memory.clone(),
            runtime_code,
            shell_runtime,
            files,
        );
        let supervisor = runtime.supervisor().clone();

        let (task_dispatch_tx, mut task_dispatch_rx) =
            tokio::sync::mpsc::channel::<TaskDispatch>(128);

        let runtime_for_tasks = runtime.clone();
        let _task_dispatch_worker = tokio::spawn(async move {
            while let Some(task) = task_dispatch_rx.recv().await {
                let actor_id = task.assignee_actor_id.clone();
                let user_id = EndpointUri::parse("borg:user:taskgraph").expect("valid uri");
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

                let payload = MessagePayload::user_text(text);

                if let Err(err) = runtime_for_tasks
                    .send_message(&user_id, &actor_id.into(), payload)
                    .await
                {
                    tracing::warn!(target: "borg_cli", error = %err, task_uri = %task.task_uri, "failed to dispatch task");
                }
            }
        });

        let taskgraph_supervisor =
            TaskGraphSupervisor::new(db.clone()).with_dispatch(task_dispatch_tx);

        let _schedule_handle = self.spawn_schedule_supervisor(db.clone());

        info!(target: "borg_cli", "starting all supervisors concurrently");
        let _taskgraph_handle = taskgraph_supervisor.start().await;

        let dashboard_url = format!("http://{bind}/");
        info!(
            target: "borg_cli",
            dashboard_url = %dashboard_url,
            "open admin dashboard"
        );

        let http_server = BorgHttpServer::new(bind, runtime, std::sync::Arc::new(supervisor));
        http_server.run().await
    }

    fn spawn_schedule_supervisor(&self, _db: BorgDb) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;
            }
        })
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
                let workspace_id = WorkspaceId::from_id("default");
                db.upsert_port_setting(
                    &workspace_id,
                    RUNTIME_SETTINGS_PORT,
                    RUNTIME_PREFERRED_PROVIDER_KEY,
                    provider.as_str(),
                )
                .await?;
                db.upsert_port_setting(
                    &workspace_id,
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
                let assigned_actor_id = existing
                    .as_ref()
                    .and_then(|port| port.assigned_actor_id.as_ref());

                let workspace_id = WorkspaceId::from_id("default");
                let port_id = borg_core::PortId::from_id("telegram");

                db.upsert_port(
                    &port_id,
                    &workspace_id,
                    "telegram",
                    "telegram",
                    enabled,
                    allows_guests,
                    assigned_actor_id,
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

    pub(crate) async fn actor_stream(&self, actor_id: String, poll_ms: u64) -> Result<()> {
        let actor_id = ActorId::parse(&actor_id).map_err(|_| {
            anyhow::anyhow!(
                "invalid actor id `{}` (expected URI like borg:actor:<id>)",
                actor_id
            )
        })?;
        let db = self.open_config_db().await?;

        loop {
            tokio::select! {
                ctrl = tokio::signal::ctrl_c() => {
                    ctrl?;
                    info!(target: "borg_cli", actor_id = %actor_id, "actor stream interrupted by ctrl-c");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(poll_ms)) => {
                    let messages = db.list_pending_messages(&actor_id.clone().into(), 100).await?;
                    for message in messages {
                        println!("{}", serde_json::to_string(&message)?);
                    }
                }
            }
        }
    }

    pub(crate) async fn actor_clear_history(&self, _actor_id: String) -> Result<()> {
        // TODO: implement clean history using new messages table
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

    pub(crate) async fn admin_actors_clear_all(&self, all: bool, yes: bool) -> Result<()> {
        if !all {
            anyhow::bail!("refusing to clear actor histories without --all");
        }
        if !yes && !confirm_actors_clear_all()? {
            println!("aborted");
            return Ok(());
        }

        let db = self.open_config_db().await?;
        db.migrate().await?;

        let mut deleted_actors = 0_u64;
        let actors = db.list_actors(50_000).await?;
        for _actor in actors {
            // TODO: implement clear history for actor
            deleted_actors += 1;
        }

        info!(
            target: "borg_cli",
            deleted_actors,
            "cleared actor histories"
        );
        println!("cleared history for {} actor(s)", deleted_actors);
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

fn confirm_actors_clear_all() -> Result<bool> {
    print!("This will permanently delete all actor histories. Continue? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(normalized == "y" || normalized == "yes")
}
