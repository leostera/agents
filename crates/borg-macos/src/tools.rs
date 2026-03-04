use anyhow::{Result, anyhow};
use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::engine::MacOsRuntime;

#[derive(Debug, Clone, Deserialize)]
struct ListShortcutsArgs {
    #[serde(default)]
    folder_name: Option<String>,
    #[serde(default)]
    folders: Option<bool>,
    #[serde(default)]
    show_identifiers: Option<bool>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunShortcutArgs {
    shortcut_name: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunAppleScriptTemplateArgs {
    template_id: String,
    #[serde(default)]
    parameters: Value,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunAppleScriptRawArgs {
    script: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ShowNotificationArgs {
    title: String,
    body: String,
    #[serde(default)]
    subtitle: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct NotifyStatusArgs {
    title: String,
    body: String,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    iphone_shortcut_name: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenArgs {
    target: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct SayArgs {
    text: String,
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "MacOS-listShortcuts".to_string(),
            description:
                "List shortcuts available on this macOS machine. Useful for discovery before invoking a shortcut by exact name."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "folder_name": { "type": "string", "description": "Optional shortcuts folder name (or `none`)." },
                    "folders": { "type": "boolean", "description": "When true, list folders instead of shortcuts." },
                    "show_identifiers": { "type": "boolean", "description": "When true, include shortcut UUID identifiers in stdout." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-runShortcut".to_string(),
            description: "Run a named shortcut on macOS using the `shortcuts` CLI.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "shortcut_name": { "type": "string", "description": "Shortcut display name to run." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["shortcut_name"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-runAppleScriptTemplate".to_string(),
            description: "Run a vetted AppleScript template with typed parameters. Safer default than raw AppleScript.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "template_id": { "type": "string", "description": "Template identifier, for example `system.display_notification`." },
                    "parameters": { "type": "object", "description": "Template parameters object." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["template_id"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-runAppleScriptRaw".to_string(),
            description: "Run raw AppleScript through `osascript`. This is policy-gated and disabled by default.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": "Raw AppleScript source." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["script"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-showNotification".to_string(),
            description: "Display a macOS notification banner.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "subtitle": { "type": "string" },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["title", "body"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-notifyStatus".to_string(),
            description:
                "Publish a Borg status notification locally, with optional iPhone relay by running a user-provided shortcut."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "severity": { "type": "string", "description": "Optional severity label (info, warning, error)." },
                    "iphone_shortcut_name": { "type": "string", "description": "Optional shortcut used to relay this event to iPhone-visible channels." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["title", "body"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-open".to_string(),
            description:
                "Open a target on macOS using the `open` command (URL, app bundle name/path, or file path)."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string" },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "MacOS-say".to_string(),
            description: "Speak text aloud on macOS using the `say` command.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "voice": { "type": "string", "description": "Optional macOS voice name." },
                    "timeout_seconds": { "type": "number", "description": "Optional execution timeout override." }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn build_macos_toolchain(
    runtime: MacOsRuntime,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let mut builder = Toolchain::builder();

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-listShortcuts")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<ListShortcutsArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.list_shortcuts(
                        request.arguments.folder_name.as_deref(),
                        request.arguments.folders.unwrap_or(false),
                        request.arguments.show_identifiers.unwrap_or(true),
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-runShortcut")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<RunShortcutArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.run_shortcut(
                        &request.arguments.shortcut_name,
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-runAppleScriptTemplate")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<RunAppleScriptTemplateArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.run_applescript_template(
                        &request.arguments.template_id,
                        &request.arguments.parameters,
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-runAppleScriptRaw")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<RunAppleScriptRawArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.run_applescript_raw(
                        &request.arguments.script,
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-showNotification")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<ShowNotificationArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.show_notification(
                        &request.arguments.title,
                        &request.arguments.body,
                        request.arguments.subtitle.as_deref(),
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-open")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<OpenArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.open_target(
                        &request.arguments.target,
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-notifyStatus")?,
        Some(execution_output_schema()),
        {
            let runtime = runtime.clone();
            move |request: borg_agent::ToolRequest<NotifyStatusArgs>| {
                let runtime = runtime.clone();
                async move {
                    let out = runtime.notify_status(
                        &request.arguments.title,
                        &request.arguments.body,
                        request.arguments.severity.as_deref(),
                        request.arguments.iphone_shortcut_name.as_deref(),
                        request.arguments.timeout_seconds,
                    )?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: out.result,
                            duration: out.duration,
                        },
                    })
                }
            }
        },
    ))?;

    builder = builder.add_tool(Tool::new_transcoded(
        required_default_tool_spec("MacOS-say")?,
        Some(execution_output_schema()),
        move |request: borg_agent::ToolRequest<SayArgs>| {
            let runtime = runtime.clone();
            async move {
                let out = runtime.say_text(
                    &request.arguments.text,
                    request.arguments.voice.as_deref(),
                    request.arguments.timeout_seconds,
                )?;
                Ok(ToolResponse {
                    content: ToolResultData::Execution {
                        result: out.result,
                        duration: out.duration,
                    },
                })
            }
        },
    ))?;

    builder.build()
}

fn execution_output_schema() -> Value {
    json!({
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
    })
}

fn required_default_tool_spec(name: &str) -> Result<ToolSpec> {
    default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| anyhow!("missing default tool spec `{}`", name))
}
