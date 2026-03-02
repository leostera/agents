pub mod codemode;
pub mod memory;
pub mod shell;
pub mod taskgraph;

use anyhow::Result;
use clap::Subcommand;
use serde_json::{Value, json};

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum ToolsCommand {
    #[command(about = "List available tool namespaces and commands")]
    List,
    #[command(about = "CodeMode tools for API discovery and JavaScript execution")]
    Codemode {
        #[command(subcommand)]
        cmd: codemode::CodeModeCommand,
    },
    #[command(about = "ShellMode tool for host shell command execution")]
    Shell {
        #[command(subcommand)]
        cmd: shell::ShellCommand,
    },
    #[command(about = "Memory tools for facts, entities, schema, and search")]
    Memory {
        #[command(subcommand)]
        cmd: memory::MemoryToolsCommand,
    },
    #[command(about = "TaskGraph tools for task lifecycle, structure, and review flows")]
    Taskgraph {
        #[command(subcommand)]
        cmd: taskgraph::TaskGraphCommand,
    },
}

pub async fn run(app: &BorgCliApp, cmd: ToolsCommand) -> Result<()> {
    let output = match cmd {
        ToolsCommand::List => catalog(),
        ToolsCommand::Codemode { cmd } => codemode::run(cmd).await?,
        ToolsCommand::Shell { cmd } => shell::run(cmd).await?,
        ToolsCommand::Memory { cmd } => memory::run(app, cmd).await?,
        ToolsCommand::Taskgraph { cmd } => taskgraph::run(app, cmd).await?,
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn catalog() -> Value {
    json!({
        "ok": true,
        "namespaces": {
            "codemode": codemode::command_names(),
            "shell": shell::command_names(),
            "memory": memory::command_names(),
            "taskgraph": taskgraph::command_names(),
        }
    })
}
