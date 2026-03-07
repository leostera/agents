mod commands;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

pub fn command_names() -> Vec<&'static str> {
    commands::all()
        .iter()
        .map(|command| command.cli_name)
        .collect()
}

pub async fn run(command: &str, payload: Value) -> Result<Value> {
    let mapping = commands::all()
        .iter()
        .find(|item| item.cli_name == command)
        .ok_or_else(|| anyhow!("unknown codemode command: {}", command))?;

    let toolchain = crate::build_code_mode_toolchain(crate::CodeModeRuntime::default())?;
    let response = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: format!("cli-codemode-{}", command),
            tool_name: mapping.tool_name.to_string(),
            arguments: payload.into(),
        })
        .await?;

    Ok(json!({
        "ok": true,
        "namespace": "codemode",
        "command": command,
        "tool": mapping.tool_name,
        "output": response.output
    }))
}
