use anyhow::{Context, Result};
use borg_agent::{BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::engine::ShellModeRuntime;
use crate::types::ShellModeContext;

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "ShellMode-executeCommand".to_string(),
        description: "Execute a shell command on the host system. Use this for CLI operations like file inspection (ls, cat, find), version control (git), build tools (cargo, npm), or any other shell-based operations. Returns stdout, stderr, and exit code.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "hint": {
                    "type": "string",
                    "description": "Human-readable description of what the command does"
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

pub fn build_shell_mode_toolchain(runtime: ShellModeRuntime) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let execute_spec = default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == "ShellMode-executeCommand")
        .context("missing ShellMode-executeCommand tool spec")?;

    let tool = Tool::new(
        execute_spec,
        Some(json!({
            "type": "object",
            "properties": {
                "Execution": {
                    "type": "object",
                    "properties": {
                        "result": {},
                        "duration": {
                            "type": "object",
                            "properties": {
                                "secs": { "type": "number" },
                                "nanos": { "type": "number" }
                            },
                            "required": ["secs", "nanos"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["result", "duration"],
                    "additionalProperties": false
                }
            },
            "required": ["Execution"],
            "additionalProperties": false
        })),
        move |request| {
            let runtime = runtime.clone();
            async move {
                let command = request
                    .arguments
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        anyhow::anyhow!("ShellMode-executeCommand tool requires command")
                    })?
                    .to_string();

                let timeout_seconds = request
                    .arguments
                    .get("timeout_seconds")
                    .and_then(Value::as_u64);

                let working_directory = request
                    .arguments
                    .get("working_directory")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);

                let context = ShellModeContext::default()
                    .with_timeout(timeout_seconds.unwrap_or(30))
                    .with_working_directory(
                        working_directory.unwrap_or_else(|| PathBuf::from(".")),
                    );

                let result = runtime.execute(&command, context)?;

                Ok(ToolResponse {
                    content: ToolResultData::Execution {
                        result: result.result_json,
                        duration: result.duration,
                    },
                })
            }
        },
    );

    Toolchain::builder().add_tool(tool)?.build()
}
