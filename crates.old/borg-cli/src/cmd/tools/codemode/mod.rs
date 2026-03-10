mod execute_code;
mod search_apis;

use anyhow::Result;
use clap::Subcommand;
use serde_json::{Value, json};

use super::decode_tool_response;

#[derive(Subcommand, Debug)]
pub enum CodeModeCommand {
    #[command(about = "List CodeMode commands")]
    List,
    #[command(about = "Execute JavaScript in CodeMode runtime; use search-apis first")]
    ExecuteCode(execute_code::ExecuteCodeArgs),
    #[command(about = "Search APIs available in Borg SDK type definitions")]
    SearchApis(search_apis::SearchApisArgs),
}

pub fn command_names() -> Vec<&'static str> {
    vec!["execute-code", "search-apis"]
}

pub async fn run(cmd: CodeModeCommand) -> Result<Value> {
    match cmd {
        CodeModeCommand::List => Ok(json!({
            "ok": true,
            "namespace": "codemode",
            "commands": command_names(),
        })),
        CodeModeCommand::ExecuteCode(args) => execute_code::run(args).await,
        CodeModeCommand::SearchApis(args) => search_apis::run(args).await,
    }
}

async fn run_tool(command: &str, tool_name: &str, payload: &str) -> Result<Value> {
    let arguments: Value = serde_json::from_str(payload)
        .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, payload))?;
    let toolchain =
        borg_codemode::build_code_mode_toolchain(borg_codemode::CodeModeRuntime::default())?;
    let response = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: format!("cli-codemode-{}", command),
            tool_name: tool_name.to_string(),
            arguments: arguments.into(),
        })
        .await?;

    decode_tool_response(response)
}

pub(super) async fn run_execute_code(payload: &str) -> Result<Value> {
    run_tool("execute-code", "CodeMode-executeCode", payload).await
}

pub(super) async fn run_search_apis(payload: &str) -> Result<Value> {
    run_tool("search-apis", "CodeMode-searchApis", payload).await
}
