use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::{Value, json};

#[derive(Subcommand, Debug)]
pub enum ShellCommand {
    #[command(about = "List ShellMode commands")]
    List,
    #[command(about = "Execute a shell command on the host system")]
    ExecuteCommand(CommandArgs),
}

#[derive(Args, Debug)]
#[command(about = "Run a shell command with optional timeout and working directory")]
pub struct CommandArgs {
    #[arg(long, help = "Shell command to execute")]
    pub command: Option<String>,
    #[arg(long, help = "Human-readable description of the command")]
    pub hint: Option<String>,
    #[arg(long, help = "Optional timeout override in seconds")]
    pub timeout_seconds: Option<u64>,
    #[arg(long, help = "Optional working directory override")]
    pub working_directory: Option<String>,
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

pub fn command_names() -> Vec<&'static str> {
    vec!["execute-command"]
}

pub async fn run(cmd: ShellCommand) -> Result<Value> {
    match cmd {
        ShellCommand::List => Ok(json!({
            "ok": true,
            "namespace": "shell",
            "commands": command_names(),
        })),
        ShellCommand::ExecuteCommand(args) => {
            let payload = if let Some(raw) = args.payload_json {
                raw
            } else {
                serde_json::to_string(&json!({
                    "command": args.command.unwrap_or_default(),
                    "hint": args.hint.unwrap_or_default(),
                    "timeout_seconds": args.timeout_seconds,
                    "working_directory": args.working_directory,
                }))?
            };
            run_tool("execute-command", "ShellMode-executeCommand", &payload).await
        }
    }
}

async fn run_tool(command: &str, tool_name: &str, payload: &str) -> Result<Value> {
    let arguments: Value = serde_json::from_str(payload)
        .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, payload))?;
    let toolchain =
        borg_shellmode::build_shell_mode_toolchain(borg_shellmode::ShellModeRuntime::new())?;
    let response = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: format!("cli-shell-{}", command),
            tool_name: tool_name.to_string(),
            arguments: arguments.into(),
        })
        .await?;

    Ok(json!({
        "ok": true,
        "namespace": "shell",
        "command": command,
        "tool": tool_name,
        "content": response.content
    }))
}
