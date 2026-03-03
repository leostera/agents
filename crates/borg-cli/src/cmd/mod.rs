mod clockwork;
mod infer;
mod models;
mod ports;
mod providers;
pub mod tools;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;

use crate::app::{BorgCliApp, DEFAULT_HTTP_BIND, DEFAULT_ONBOARD_PORT, DEFAULT_POLL_INTERVAL_MS};

#[derive(Parser, Debug)]
#[command(
    name = "borg",
    about = "Borg runtime CLI",
    long_about = "Manage Borg services, sessions, memory, and configuration."
)]
pub struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Initialize local Borg storage and onboard assets")]
    Init {
        #[arg(long, default_value_t = DEFAULT_ONBOARD_PORT, help = "Port for onboarding web server")]
        onboard_port: u16,
    },
    #[command(about = "Start the Borg runtime services")]
    Start {
        #[arg(long, default_value = DEFAULT_HTTP_BIND, help = "API bind address in host:port form")]
        bind: String,
    },
    #[command(about = "Session utilities")]
    Session {
        #[command(subcommand)]
        cmd: SessionCommand,
    },
    #[command(about = "Memory maintenance commands")]
    Memory {
        #[command(subcommand)]
        cmd: MemoryCommand,
    },
    #[command(about = "Set persisted runtime configuration values")]
    Config {
        #[command(subcommand)]
        cmd: ConfigCommand,
    },
    #[command(about = "Runtime administration commands")]
    Admin {
        #[command(subcommand)]
        cmd: AdminCommand,
    },
    #[command(about = "Invoke subsystem tools directly")]
    Tools {
        #[command(subcommand)]
        cmd: tools::ToolsCommand,
    },
    #[command(about = "Providers CRUD commands")]
    Providers {
        #[command(subcommand)]
        cmd: providers::ProvidersCommand,
    },
    #[command(about = "Ports CRUD commands")]
    Ports {
        #[command(subcommand)]
        cmd: ports::PortsCommand,
    },
    #[command(about = "Model download and cache commands")]
    Models {
        #[command(subcommand)]
        cmd: models::ModelsCommand,
    },
    #[command(about = "Clockwork jobs CRUD commands")]
    Clockwork {
        #[command(subcommand)]
        cmd: clockwork::ClockworkCommand,
    },
    #[command(about = "Local embedded inference commands")]
    Infer {
        #[command(subcommand)]
        cmd: infer::InferCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    #[command(about = "Persist one config key/value pair")]
    Set {
        #[arg(help = "Config key (for example providers.openai or ports.telegram)")]
        key: String,
        #[arg(help = "Config value to persist")]
        value: String,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCommand {
    #[command(about = "Stream messages for a session")]
    Stream {
        #[arg(help = "Session URI (for example borg:session:...)")]
        session_id: String,
        #[arg(long, default_value_t = DEFAULT_POLL_INTERVAL_MS, help = "Poll interval in milliseconds")]
        poll_ms: u64,
    },
    #[command(about = "Delete all persisted messages for a session")]
    ClearHistory {
        #[arg(help = "Session URI to clear")]
        session_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum MemoryCommand {
    #[command(about = "Clear all memory facts and search data")]
    Clear {
        #[arg(long, help = "Skip confirmation prompt")]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum AdminCommand {
    #[command(about = "TaskGraph maintenance commands")]
    Tasks {
        #[command(subcommand)]
        cmd: AdminTasksCommand,
    },
    #[command(about = "Session maintenance commands")]
    Sessions {
        #[command(subcommand)]
        cmd: AdminSessionsCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AdminTasksCommand {
    #[command(about = "Delete all TaskGraph tasks")]
    ClearAllTasks {
        #[arg(long, help = "Skip confirmation prompt")]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum AdminSessionsCommand {
    #[command(about = "Delete all sessions and their persisted messages")]
    ClearSessions {
        #[arg(long, help = "Required safety flag: clear all sessions")]
        all: bool,
        #[arg(long, help = "Skip confirmation prompt")]
        yes: bool,
    },
}

pub async fn run(app: BorgCliApp, cli: Cli) -> Result<()> {
    match cli.cmd {
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
            AdminCommand::Sessions { cmd } => match cmd {
                AdminSessionsCommand::ClearSessions { all, yes } => {
                    app.admin_sessions_clear_all(all, yes).await
                }
            },
        },
        Command::Tools { cmd } => {
            if let Err(err) = tools::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
        Command::Providers { cmd } => {
            if let Err(err) = providers::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
        Command::Ports { cmd } => {
            if let Err(err) = ports::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
        Command::Models { cmd } => {
            if let Err(err) = models::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
        Command::Clockwork { cmd } => {
            if let Err(err) = clockwork::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
        Command::Infer { cmd } => {
            if let Err(err) = infer::run(&app, cmd).await {
                println!(
                    "{}",
                    serde_json::to_string(&json!({ "ok": false, "error": err.to_string() }))?
                );
            }
            Ok(())
        }
    }
}
