use crate::mailbox_envelope::{ActorMailboxEnvelope, ActorMailboxInput};
use anyhow::{Result, anyhow};
use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
    ToolchainBuilder, build_actor_admin_toolchain, default_actor_admin_tool_specs,
};
use borg_codemode::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain_with_context};
use borg_core::Uri;
use borg_db::BorgDb;
use borg_fs::{BorgFs, build_borg_fs_toolchain, default_borg_fs_tool_specs};
use borg_llm::{default_provider_admin_tool_specs, run_provider_admin_tool};
use borg_memory::{MemoryStore, build_memory_toolchain};
use borg_ports_tools::{build_port_admin_toolchain, default_port_admin_tool_specs};
use borg_schedule::build_schedule_toolchain;
use borg_shellmode::{ShellModeRuntime, build_shell_mode_toolchain};
use borg_taskgraph::{build_taskgraph_toolchain, build_taskgraph_worker_toolchain};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{Duration, Instant, sleep};

const ACTOR_RECEIVE_DEFAULT_TIMEOUT_MS: u64 = 60_000;
const ACTOR_RECEIVE_POLL_INTERVAL_MS: u64 = 100;

#[derive(Debug, Clone, Deserialize)]
struct ActorsSendMessageArgs {
    target_actor_id: String,
    text: String,
    #[serde(default)]
    in_reply_to_submission_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorsReceiveArgs {
    #[serde(default)]
    expected_submission_id: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    shell_runtime: ShellModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
    db: BorgDb,
    files: BorgFs,
    current_actor_id: Uri,
    current_user_id: Uri,
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
    let actor_messaging =
        build_actor_messaging_toolchain(db.clone(), current_actor_id, current_user_id)?;
    let port_admin = build_port_admin_toolchain(db.clone())?;
    let provider_admin = build_provider_admin_toolchain(db)?;
    code.merge(shell)?
        .merge(ltm)?
        .merge(fs_tools)?
        .merge(taskgraph)?
        .merge(schedule)?
        .merge(actor_admin)?
        .merge(actor_messaging)?
        .merge(port_admin)?
        .merge(provider_admin)
}

pub fn default_exec_admin_tool_specs() -> Vec<ToolSpec> {
    let mut out = Vec::new();
    out.extend(default_actor_admin_tool_specs());
    out.extend(default_actor_messaging_tool_specs());
    out.extend(default_port_admin_tool_specs());
    out.extend(default_borg_fs_tool_specs());
    out.extend(
        default_provider_admin_tool_specs()
            .into_iter()
            .map(|spec| ToolSpec {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            }),
    );
    out
}

fn default_actor_messaging_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "Actors-sendMessage".to_string(),
            description: "Send a message from the current actor to another actor. If an inbound message contains `ACTOR_MESSAGE_META` with `reply_target_actor_id`, use this tool to reply to that actor and include `in_reply_to_submission_id` when provided."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_actor_id": {
                        "type": "string",
                        "format": "uri",
                        "description": "Destination actor URI. For replies, set this to `reply_target_actor_id` from `ACTOR_MESSAGE_META`."
                    },
                    "text": {
                        "type": "string",
                        "description": "Message text to send to the target actor."
                    },
                    "in_reply_to_submission_id": {
                        "type": "string",
                        "format": "uri",
                        "description": "Submission/message URI being replied to. Use `submission_id` from `ACTOR_MESSAGE_META` when present."
                    }
                },
                "required": ["target_actor_id", "text"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Actors-receive".to_string(),
            description:
                "Wait for the next actor reply message for this actor, optionally filtered by submission id. Reply messages include `source_actor_id`; when responding back, call `Actors-sendMessage`."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "expected_submission_id": {
                        "type": "string",
                        "format": "uri",
                        "description": "Only return replies that reference this submission/message URI."
                    },
                    "timeout_ms": { "type": "integer", "minimum": 1, "maximum": 300000 }
                },
                "additionalProperties": false
            }),
        },
    ]
}

fn build_actor_messaging_toolchain(
    db: BorgDb,
    current_actor_id: Uri,
    current_user_id: Uri,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let mut builder = ToolchainBuilder::new();

    let send_spec = default_actor_messaging_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "Actors-sendMessage")
        .ok_or_else(|| anyhow!("missing Actors-sendMessage spec"))?;
    let receive_spec = default_actor_messaging_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "Actors-receive")
        .ok_or_else(|| anyhow!("missing Actors-receive spec"))?;

    let db_send = db.clone();
    let sender_actor_id = current_actor_id.clone();
    let sender_user_id = current_user_id.clone();
    builder = builder.add_tool(Tool::new_transcoded(
        send_spec,
        None,
        move |request: borg_agent::ToolRequest<ActorsSendMessageArgs>| {
            let db = db_send.clone();
            let sender_actor_id = sender_actor_id.clone();
            let sender_user_id = sender_user_id.clone();
            async move {
                let target_actor_id = Uri::parse(request.arguments.target_actor_id.trim())?;
                if db.get_actor(&target_actor_id).await?.is_none() {
                    return Err(anyhow!("actor.not_found"));
                }

                let in_reply_to_submission_id = request
                    .arguments
                    .in_reply_to_submission_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(Uri::parse)
                    .transpose()?;

                let text = request.arguments.text.trim();
                if text.is_empty() {
                    return Err(anyhow!("validation_failed: missing text"));
                }
                let input = ActorMailboxInput::Chat {
                    text: text.to_string(),
                };

                let envelope = ActorMailboxEnvelope {
                    actor_id: target_actor_id.to_string(),
                    user_id: sender_user_id.to_string(),
                    port_context: crate::PortContext::Unknown,
                    input,
                };
                let payload = serde_json::to_value(envelope)?;
                let actor_message_id = db
                    .enqueue_actor_message_from_sender(
                        Some(&sender_actor_id),
                        &target_actor_id,
                        &payload,
                        None,
                        in_reply_to_submission_id.as_ref(),
                    )
                    .await?;
                let response = json!({
                    "status": "delivered",
                    "actor_message_id": actor_message_id.to_string(),
                    "submission_id": actor_message_id.to_string(),
                    "sender_actor_id": sender_actor_id.to_string(),
                    "target_actor_id": target_actor_id.to_string(),
                });
                Ok(ToolResponse {
                    output: ToolResultData::Ok(response),
                })
            }
        },
    ))?;

    let db_receive = db.clone();
    builder = builder.add_tool(Tool::new_transcoded(
        receive_spec,
        None,
        move |request: borg_agent::ToolRequest<ActorsReceiveArgs>| {
            let db = db_receive.clone();
            let current_actor_id = current_actor_id.clone();
            async move {
                let timeout_ms = request
                    .arguments
                    .timeout_ms
                    .unwrap_or(ACTOR_RECEIVE_DEFAULT_TIMEOUT_MS);
                let expected_submission_id = request
                    .arguments
                    .expected_submission_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(Uri::parse)
                    .transpose()?;

                let timeout = Duration::from_millis(timeout_ms);
                let deadline = Instant::now() + timeout;
                loop {
                    if let Some(row) = db
                        .claim_next_actor_reply_message(
                            &current_actor_id,
                            expected_submission_id.as_ref(),
                        )
                        .await?
                    {
                        let envelope: ActorMailboxEnvelope =
                            serde_json::from_value(row.payload.clone())?;
                        let text = match envelope.input {
                            ActorMailboxInput::Chat { text } => text,
                            ActorMailboxInput::Command { command } => {
                                serde_json::to_string(&command)?
                            }
                            ActorMailboxInput::Audio { file_id, .. } => file_id,
                        };
                        let _ = db.ack_actor_message(&row.actor_message_id).await;
                        let response = json!({
                            "status": "completed",
                            "actor_message_id": row.actor_message_id.to_string(),
                            "submission_id": row.actor_message_id.to_string(),
                            "in_reply_to_submission_id": row.reply_to_message_id.map(|value| value.to_string()),
                            "source_actor_id": row.sender_actor_id.map(|value| value.to_string()),
                            "text": text,
                        });
                        return Ok(ToolResponse {
                             output: ToolResultData::Ok(response),
                        });
                    }

                    if Instant::now() >= deadline {
                        return Err(anyhow!("actors.receive.timeout"));
                    }

                    sleep(Duration::from_millis(ACTOR_RECEIVE_POLL_INTERVAL_MS)).await;
                }
            }
        },
    ))?;

    builder.build()
}

fn build_provider_admin_toolchain(db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let mut builder = ToolchainBuilder::new();
    for spec in default_provider_admin_tool_specs() {
        let db = db.clone();
        let name = spec.name.clone();
        let tool = Tool::new_transcoded(
            ToolSpec {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            },
            None,
            move |request: borg_agent::ToolRequest<BorgToolCall>| {
                let db = db.clone();
                let name = name.clone();
                async move {
                    let arguments = request.arguments.to_value()?;
                    let value = run_provider_admin_tool(&db, &name, &arguments).await?;
                    Ok(ToolResponse {
                        output: ToolResultData::Ok(value),
                    })
                }
            },
        );
        builder = builder.add_tool(tool)?;
    }
    builder.build()
}
