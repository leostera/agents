use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
    ToolchainBuilder, build_actor_admin_toolchain,
};
use borg_codemode::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain_with_context};
use borg_core::{ActorId, EndpointUri, MessagePayload};
use borg_db::BorgDb;
use borg_fs::{BorgFs, build_borg_fs_toolchain};
use borg_llm::default_provider_admin_tool_specs;
use borg_memory::{MemoryStore, build_memory_toolchain};
use borg_ports_tools::{build_port_admin_toolchain, default_port_admin_tool_specs};
use borg_schedule::build_schedule_toolchain;
use borg_shellmode::{ShellModeRuntime, build_shell_mode_toolchain};
use borg_taskgraph::{build_taskgraph_toolchain, build_taskgraph_worker_toolchain};

use crate::runtime::BorgRuntime;

pub fn build_exec_toolchain_with_context(
    rt: Arc<BorgRuntime>,
    runtime: CodeModeRuntime,
    shell_runtime: ShellModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
    db: BorgDb,
    files: BorgFs,
    current_actor_id: ActorId,
    _current_user_id: EndpointUri,
    allow_task_creation: bool,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let code = build_code_mode_toolchain_with_context(runtime, context)?;
    let shell = build_shell_mode_toolchain(shell_runtime)?;
    let ltm = build_memory_toolchain(memory)?;
    let fs_tools = build_borg_fs_toolchain(files)?;
    let taskgraph = if allow_task_creation {
        build_taskgraph_toolchain(db.clone())?
    } else {
        build_taskgraph_worker_toolchain(db.clone())?
    };
    let schedule = build_schedule_toolchain(db.clone())?;
    let actor_admin = build_actor_admin_toolchain(db.clone(), current_actor_id.clone())?;
    let actor_messaging = build_actor_messaging_toolchain(rt, current_actor_id)?;
    let patch_tools = build_patch_toolchain()?;
    let port_admin = build_port_admin_toolchain(db.clone())?;
    let provider_admin = build_provider_admin_toolchain(db)?;
    code.merge(shell)?
        .merge(ltm)?
        .merge(fs_tools)?
        .merge(taskgraph)?
        .merge(schedule)?
        .merge(actor_admin)?
        .merge(actor_messaging)?
        .merge(patch_tools)?
        .merge(port_admin)?
        .merge(provider_admin)
}

pub fn default_exec_admin_tool_specs() -> Vec<ToolSpec> {
    let mut specs = Vec::new();
    specs.extend(default_port_admin_tool_specs());
    specs.extend(default_actor_messaging_tool_specs());
    specs.extend(
        default_provider_admin_tool_specs()
            .into_iter()
            .map(|s| ToolSpec {
                name: s.name,
                description: s.description,
                parameters: s.parameters,
            }),
    );
    specs
}

#[derive(Debug, Clone, Deserialize)]
struct SendMessageArgs {
    target_actor_id: serde_json::Value,
    text: String,
    #[serde(default)]
    reply_target_actor_id: Option<serde_json::Value>,
    #[serde(default)]
    submission_id: Option<String>,
    #[serde(default)]
    in_reply_to_submission_id: Option<String>,
}

fn build_actor_messaging_toolchain(
    rt: Arc<BorgRuntime>,
    current_actor_id: ActorId,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let mut builder = ToolchainBuilder::new();

    let send_spec = default_actor_messaging_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "Actors-sendMessage")
        .ok_or_else(|| anyhow!("missing Actors-sendMessage spec"))?;

    builder = builder.add_tool(Tool::new_transcoded(
        send_spec,
        None,
        move |request: borg_agent::ToolRequest<SendMessageArgs>| {
            let rt = rt.clone();
            let sender_id = current_actor_id.clone();
            async move {
                let target_id_str = match &request.arguments.target_actor_id {
                    serde_json::Value::String(s) => s.clone(),
                    val => val
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| {
                            anyhow!("invalid target_actor_id: expected string or object with 'uri'")
                        })?,
                };
                let target_id = ActorId::parse(&target_id_str)?;

                let reply_target_actor_id = match &request.arguments.reply_target_actor_id {
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(val) => val
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    None => None,
                };

                // Construct the structured actor message as per protocol
                let payload_json = json!({
                    "type": "actor_message",
                    "sender_actor_id": sender_id.as_str(),
                    "text": request.arguments.text,
                    "reply_target_actor_id": reply_target_actor_id,
                    "submission_id": request.arguments.submission_id,
                    "in_reply_to_submission_id": request.arguments.in_reply_to_submission_id,
                });

                let payload = MessagePayload::user_text(payload_json.to_string());

                rt.send_message(&sender_id.into(), &target_id.into(), payload)
                    .await?;

                Ok(ToolResponse {
                    output: ToolResultData::Ok(json!({ "status": "sent" })),
                })
            }
        },
    ))?;

    builder.build()
}

pub fn default_actor_messaging_tool_specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "Actors-sendMessage".to_string(),
        description: "Send a structured message to another actor.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "target_actor_id": { "type": "string", "format": "uri", "description": "The recipient Actor ID (borg:actor:<id>)" },
                "text": { "type": "string", "description": "The message content" },
                "reply_target_actor_id": { "type": "string", "format": "uri", "description": "Optional: Actor ID where replies should be sent" },
                "submission_id": { "type": "string", "description": "Optional: Unique ID for this message to track replies" },
                "in_reply_to_submission_id": { "type": "string", "description": "Optional: If replying to a message, include its submission_id here" }
            },
            "required": ["target_actor_id", "text"],
            "additionalProperties": false
        }),
    }]
}

fn build_patch_toolchain() -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    ToolchainBuilder::new().build()
}

fn build_provider_admin_toolchain(_db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    ToolchainBuilder::new().build()
}
