use std::{process::Command as ProcessCommand, time::Duration};

use anyhow::Result;
use borg_api::BorgApiServer;
use borg_core::borgdir::BorgDir;
use borg_db::BorgDb;
use borg_exec::ExecEngine;
use borg_ltm::MemoryStore;
use borg_onboard::OnboardServer;
use borg_rt::RuntimeEngine;
use clap::{Parser, Subcommand};
use tracing::{debug, error, info, warn};

const DEFAULT_HTTP_BIND: &str = "127.0.0.1:8080";
const DEFAULT_ONBOARD_PORT: u16 = 3777;
const DEFAULT_SCHEDULER_POLL_MS: u64 = 500;

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

    async fn start(&self, bind: String, poll_ms: u64) -> Result<()> {
        info!(target: "borg_cli", config_db = %self.borg_dir.config_db().display(), bind, poll_ms, "starting borg machine");

        let db = self.open_config_db().await?;
        let memory = MemoryStore::new(self.borg_dir.ltm_db(), self.borg_dir.search_db())?;
        let exec = ExecEngine::new(
            db.clone(),
            RuntimeEngine::default(),
            format!("worker-{}", std::process::id()),
        );

        db.migrate().await?;
        memory.migrate().await?;

        let scheduler_exec = exec.clone();
        let scheduler = tokio::spawn(async move {
            info!(target: "borg_cli", "scheduler loop started");
            loop {
                match scheduler_exec.run_once().await {
                    Ok(true) => debug!(target: "borg_cli", "scheduler processed one task"),
                    Ok(false) => debug!(target: "borg_cli", "scheduler tick had no work"),
                    Err(err) => error!(target: "borg_cli", error = %err, "scheduler tick failed"),
                }
                tokio::time::sleep(Duration::from_millis(poll_ms)).await;
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
        Command::Start { bind, poll_ms } => app.start(bind, poll_ms).await,
    }
}
