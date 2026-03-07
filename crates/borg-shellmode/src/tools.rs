use anyhow::{Context, Result};
use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::engine::ShellModeRuntime;
use crate::types::ShellModeContext;

#[derive(Debug, Clone, Deserialize)]
struct ExecuteCommandArgs {
    command: String,
    hint: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    working_directory: Option<String>,
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "ShellMode-executeCommand".to_string(),
        description: "Execute a shell command on the host system.\n\nRequired argument shape (JSON object):\n{\"command\":\"<shell command>\",\"hint\":\"<short intent>\",\"timeout_seconds\":30,\"working_directory\":\".\"}\n\nRules:\n- `command` MUST be a single string command (not an array).\n- Do not use keys like `cmd`, `timeout`, or `cwd`.\n- `timeout_seconds` and `working_directory` are optional.\n\nUse this for CLI operations like file inspection (ls, cat, find), version control (git), and build tooling.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Single shell command string to execute (for example: `which rg`)"
                },
                "hint": {
                    "type": "string",
                    "description": "Short human-readable intent for the command"
                },
                "timeout_seconds": {
                    "type": "number",
                    "description": "Optional timeout override in seconds (default: 30)"
                },
                "working_directory": {
                    "type": "string",
                    "description": "Optional working directory override"
                }
            },
            "required": ["command", "hint"],
            "additionalProperties": false
        }),
    }]
}

pub fn build_shell_mode_toolchain(
    runtime: ShellModeRuntime,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let execute_spec = default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == "ShellMode-executeCommand")
        .context("missing ShellMode-executeCommand tool spec")?;

    let tool = Tool::new_transcoded(
        execute_spec,
        Some(json!({
            "type": "object",
            "properties": {
                "result": {},
                "duration_ms": { "type": "integer", "minimum": 0 }
            },
            "required": ["result", "duration_ms"],
            "additionalProperties": false
        })),
        move |request: borg_agent::ToolRequest<ExecuteCommandArgs>| {
            let runtime = runtime.clone();
            async move {
                let command = request.arguments.command.trim().to_string();
                if command.is_empty() {
                    return Err(anyhow::anyhow!(
                        "ShellMode-executeCommand tool requires command"
                    ));
                }
                let _hint = request.arguments.hint;
                let timeout_seconds = request.arguments.timeout_seconds;
                let working_directory = request.arguments.working_directory.map(PathBuf::from);

                let context = ShellModeContext::default()
                    .with_timeout(timeout_seconds.unwrap_or(30))
                    .with_working_directory(
                        working_directory.unwrap_or_else(|| PathBuf::from(".")),
                    );

                let result = runtime.execute(&command, context)?;

                Ok(ToolResponse {
                    output: ToolResultData::Ok(json!({
                        "result": result.result,
                        "duration_ms": result.duration.as_millis(),
                    })),
                })
            }
        },
    );

    Toolchain::builder().add_tool(tool)?.build()
}
