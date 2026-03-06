pub mod codemode;
pub mod fs;
pub mod memory;
pub mod shell;
pub mod taskgraph;

use anyhow::Result;
use borg_agent::{BorgToolResult, ToolResponse, ToolResultData};
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
    #[command(about = "BorgFS tools for listing, reading, writing, and deleting files")]
    Fs {
        #[command(subcommand)]
        cmd: fs::FsToolsCommand,
    },
}

pub async fn run(app: &BorgCliApp, cmd: ToolsCommand) -> Result<()> {
    let output = match cmd {
        ToolsCommand::List => catalog(),
        ToolsCommand::Codemode { cmd } => codemode::run(cmd).await?,
        ToolsCommand::Shell { cmd } => shell::run(cmd).await?,
        ToolsCommand::Fs { cmd } => fs::run(app, cmd).await?,
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
            "fs": fs::command_names(),
        }
    })
}

pub(super) fn decode_tool_response(response: ToolResponse<BorgToolResult>) -> Result<Value> {
    match response.content {
        ToolResultData::Text(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => Ok(value),
            Err(_) => Ok(json!({ "text": text })),
        },
        ToolResultData::Capabilities(capabilities) => Ok(json!({ "capabilities": capabilities })),
        ToolResultData::Execution { result, duration } => Ok(json!({
            "result": result.to_value()?,
            "duration_ms": duration.as_millis(),
        })),
        ToolResultData::Error { message } => Ok(json!({ "error": message })),
    }
}
