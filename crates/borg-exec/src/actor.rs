use anyhow::{Result, anyhow};
use borg_agent::{
    ActorRunResult, ActorThread, Agent, BorgToolCall, BorgToolResult, Message, ToolCallRecord,
    Toolchain,
};
use borg_core::Uri;
use borg_llm::{ReasoningEffort, TranscriptionRequest};
use chrono::Utc;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::mailbox::ActorCommand;
use crate::message::{
    ActorOutboundMessage, ActorOutput, BorgCommand, BorgInput, BorgMessage, ToolCallSummary,
};
use crate::port_context::{PortContext, TelegramContext};
use crate::runtime::BorgRuntime;

const TOOLCHAIN_CACHE_TTL: Duration = Duration::from_secs(60);
const CONTEXT_COMPACTION_KEEP_MESSAGES: usize = 24;
const CONTEXT_OVERFLOW_PROGRESS_MESSAGE: &str =
    "Context window exceeded provider limits. Compacting context and retrying...";

struct CachedToolchain {
    user_id: Uri,
    actor_id: Uri,
    built_at: Instant,
    toolchain: Arc<Toolchain<BorgToolCall, BorgToolResult>>,
}

pub struct ActorHandle {
    pub actor_id: Uri,
    pub mailbox: mpsc::Sender<ActorCommand>,
    pub task: Arc<JoinHandle<()>>,
}

impl Clone for ActorHandle {
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id.clone(),
            mailbox: self.mailbox.clone(),
            task: Arc::clone(&self.task),
        }
    }
}

impl ActorHandle {
    pub async fn spawn(actor_id: Uri, runtime: Arc<BorgRuntime>) -> Result<Self> {
        let actor = Actor::new(actor_id.clone(), runtime).await?;
        let mailbox = actor.mailbox();

        let task = Arc::new(tokio::spawn(async move {
            actor.run().await;
        }));

        Ok(Self {
            actor_id,
            mailbox,
            task,
        })
    }
}

struct ActorState {
    actor_thread: ActorThread<BorgToolCall, BorgToolResult>,
    actor_id: Uri,
    current_reasoning_effort: Option<ReasoningEffort>,
    cached_toolchain: Option<CachedToolchain>,
}

struct Actor {
    actor_id: Uri,
    mailbox: mpsc::Sender<ActorCommand>,
    rx: mpsc::Receiver<ActorCommand>,
    runtime: Arc<BorgRuntime>,
    actor_state: Option<ActorState>,
}

impl Actor {
    async fn new(actor_id: Uri, runtime: Arc<BorgRuntime>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        Ok(Self {
            actor_id,
            mailbox: tx,
            rx,
            runtime,
            actor_state: None,
        })
    }

    fn mailbox(&self) -> mpsc::Sender<ActorCommand> {
        self.mailbox.clone()
    }

    async fn run(mut self) {
        debug!("actor {} loop started", self.actor_id);
        loop {
            debug!(target: "borg_exec", actor_id = %self.actor_id, "awaiting message");
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    match cmd {
                        ActorCommand::Cast { actor_message_id, sender_actor_id, msg } => {
                            let sender = sender_actor_id
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| "user".to_string());
                            debug!(
                                target: "borg_exec",
                                actor_id = %self.actor_id,
                                actor_message_id = %actor_message_id,
                                sender_actor_id = %sender,
                                "message received"
                            );
                            let result = self
                                .process_message(
                                    msg,
                                    sender_actor_id.as_ref(),
                                    Some(&actor_message_id),
                                    None,
                                )
                                .await;
                            if let Err(err) = &result {
                                let _ = self
                                    .runtime
                                    .db
                                    .fail_actor_message(&actor_message_id, &err.to_string())
                                    .await;
                            } else {
                                let _ = self.runtime.db.ack_actor_message(&actor_message_id).await;
                            }
                        }
                        ActorCommand::Call {
                            actor_message_id,
                            sender_actor_id,
                            msg,
                            progress_tx,
                            response_tx,
                        } => {
                            let sender = sender_actor_id
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| "user".to_string());
                            debug!(
                                target: "borg_exec",
                                actor_id = %self.actor_id,
                                actor_message_id = %actor_message_id,
                                sender_actor_id = %sender,
                                "message received"
                            );
                            let result = self
                                .process_message(
                                    msg,
                                    sender_actor_id.as_ref(),
                                    Some(&actor_message_id),
                                    progress_tx,
                                )
                                .await;
                            if let Err(err) = &result {
                                let _ = self
                                    .runtime
                                    .db
                                    .fail_actor_message(&actor_message_id, &err.to_string())
                                    .await;
                            } else {
                                let _ = self.runtime.db.ack_actor_message(&actor_message_id).await;
                            }
                            let _ = response_tx.send(result);
                        }
                        ActorCommand::Terminate => {
                            info!("actor {} terminating", self.actor_id);
                            break;
                        }
                    }
                }
                else => break,
            }
        }

        debug!("actor {} loop ended", self.actor_id);
    }

    async fn process_message(
        &mut self,
        msg: BorgMessage,
        sender_actor_id: Option<&Uri>,
        actor_message_id: Option<&Uri>,
        progress_tx: Option<mpsc::Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
    ) -> Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        let actor_runtime_id = self.actor_id.clone();

        if self.actor_state.is_none() {
            let actor_id = self.resolve_execution_actor_id().await?;
            let actor_thread = match self.create_actor_thread(&actor_id).await {
                Ok(actor_thread) => actor_thread,
                Err(err)
                    if matches!(msg.input, BorgInput::Command(_))
                        && err.to_string().contains("model not configured") =>
                {
                    self.create_command_actor_thread(&actor_id).await?
                }
                Err(err) => return Err(err),
            };
            let current_reasoning_effort =
                self.load_actor_reasoning_effort(&actor_runtime_id).await?;
            self.actor_state = Some(ActorState {
                actor_thread,
                actor_id,
                current_reasoning_effort,
                cached_toolchain: None,
            });
        }

        let drop_after = matches!(&msg.input, BorgInput::Command(BorgCommand::ResetContext));
        let mut state = self
            .actor_state
            .take()
            .ok_or_else(|| anyhow!("actor state missing for {}", actor_runtime_id))?;

        let result = match &msg.input {
            BorgInput::Chat { text } => {
                self.process_chat_message(
                    &mut state,
                    &msg,
                    sender_actor_id,
                    actor_message_id,
                    text,
                    progress_tx.as_ref(),
                )
                .await
            }
            BorgInput::Audio {
                file_id,
                mime_type,
                duration_ms,
                language_hint,
            } => {
                self.process_audio_message(
                    &mut state,
                    &msg,
                    actor_message_id,
                    file_id,
                    mime_type.as_deref(),
                    *duration_ms,
                    language_hint.as_deref(),
                    progress_tx.as_ref(),
                )
                .await
            }
            BorgInput::Command(command) => {
                self.process_control_command(&mut state, &msg, command)
                    .await
            }
        };

        if !drop_after {
            self.actor_state = Some(state);
        }

        result
    }

    async fn create_actor_thread(
        &self,
        actor_id: &Uri,
    ) -> Result<ActorThread<BorgToolCall, BorgToolResult>> {
        self.runtime
            .actor_context_manager
            .actor_thread_for_task(Some(actor_id))
            .await
    }

    async fn create_command_actor_thread(
        &self,
        actor_id: &Uri,
    ) -> Result<ActorThread<BorgToolCall, BorgToolResult>> {
        let actor = self
            .runtime
            .db
            .get_actor(actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", actor_id))?;
        let agent = Agent::new(actor_id.clone()).with_system_prompt(actor.system_prompt);
        ActorThread::new(actor_id.clone(), agent, self.runtime.db.clone()).await
    }

    async fn process_chat_message(
        &self,
        state: &mut ActorState,
        msg: &BorgMessage,
        sender_actor_id: Option<&Uri>,
        actor_message_id: Option<&Uri>,
        text: &str,
        progress_tx: Option<&mpsc::Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
    ) -> Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        state.actor_thread.agent.reasoning_effort = state.current_reasoning_effort;
        let content = match sender_actor_id {
            Some(sender_actor_id) => {
                let submission_id = actor_message_id
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "unknown".to_string());
                format!(
                    "ACTOR_MESSAGE_META {{\"sender_actor_id\":\"{sender_actor_id}\",\"reply_target_actor_id\":\"{sender_actor_id}\",\"submission_id\":\"{submission_id}\"}}\n\n{text}"
                )
            }
            None => text.to_string(),
        };
        state
            .actor_thread
            .add_message(Message::User { content })
            .await?;

        self.process_text_turn(state, msg, actor_message_id, progress_tx).await
    }

    async fn process_audio_message(
        &self,
        state: &mut ActorState,
        msg: &BorgMessage,
        actor_message_id: Option<&Uri>,
        file_id: &Uri,
        mime_type_hint: Option<&str>,
        _duration_ms: Option<u64>,
        language_hint: Option<&str>,
        progress_tx: Option<&mpsc::Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
    ) -> Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        state.actor_thread.agent.reasoning_effort = state.current_reasoning_effort;
        let (file_record, audio_bytes) = self.runtime.files.read_all(file_id).await?;
        let mime_type = mime_type_hint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(file_record.content_type.as_str())
            .to_string();

        let llm = self.runtime.llm().await?;
        let transcript = llm
            .audio_transcription(&TranscriptionRequest {
                audio: audio_bytes,
                mime_type: mime_type.clone(),
                model: None,
                language: language_hint.map(str::to_string),
                prompt: None,
            })
            .await?;
        if transcript.trim().is_empty() {
            return Err(anyhow!("audio transcription produced an empty transcript"));
        }

        state
            .actor_thread
            .add_message(Message::UserAudio {
                file_id: file_id.clone(),
                transcript: transcript.clone(),
                created_at: Utc::now().to_rfc3339(),
            })
            .await?;

        self.process_text_turn(state, msg, actor_message_id, progress_tx).await
    }

    async fn process_text_turn(
        &self,
        state: &mut ActorState,
        msg: &BorgMessage,
        actor_message_id: Option<&Uri>,
        progress_tx: Option<&mpsc::Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
    ) -> Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        state.actor_id = self.resolve_execution_actor_id().await?;
        state.actor_thread.agent = self
            .runtime
            .actor_context_manager
            .resolve_agent_for_turn(&state.actor_id)
            .await?;
        state.actor_thread.agent.reasoning_effort = state.current_reasoning_effort;

        let llm = self.runtime.llm().await?;
        let toolchain = self.toolchain_for_turn(state, msg).await?;

        let mut tool_event_tx: Option<
            mpsc::UnboundedSender<ToolCallRecord<BorgToolCall, BorgToolResult>>,
        > = None;
        let mut tool_event_task = None;
        let actor_message_id_for_task = actor_message_id.cloned();
        if let Some(progress_tx) = progress_tx {
            let (tx, mut rx) = mpsc::unbounded_channel();
            tool_event_tx = Some(tx);
            let actor_id = state.actor_thread.actor_id.clone();
            let port_context = msg.port_context.clone();
            let progress_tx = progress_tx.clone();
            tool_event_task = Some(tokio::spawn(async move {
                while let Some(call) = rx.recv().await {
                    if progress_tx
                        .send(ActorOutput {
                            actor_message_id: actor_message_id_for_task.clone(),
                            actor_id: actor_id.clone(),
                            outbound_messages: Vec::new(),
                            tool_calls: vec![ToolCallSummary {
                                tool_name: call.tool_name,
                                arguments: call.arguments,
                                output: call.output,
                            }],
                            port_context: port_context.clone(),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }

        let mut run_result = state
            .actor_thread
            .agent
            .clone()
            .run_with_tool_events(
                &mut state.actor_thread,
                &llm,
                toolchain.as_ref(),
                tool_event_tx.as_ref(),
            )
            .await;

        if let ActorRunResult::ActorError(err) = &run_result
            && is_context_window_overflow_error(err)
        {
            warn!(
                target: "borg_exec",
                actor_id = %state.actor_thread.actor_id,
                error = %err,
                "actor turn exceeded context window; compacting and retrying once"
            );

            if let Some(progress_tx) = progress_tx {
                let _ = progress_tx
                    .send(ActorOutput {
                        actor_message_id: actor_message_id.cloned(),
                        actor_id: state.actor_thread.actor_id.clone(),
                        outbound_messages: make_port_reply_messages(
                            &msg.port_context,
                            Some(CONTEXT_OVERFLOW_PROGRESS_MESSAGE.to_string()),
                        ),
                        tool_calls: Vec::new(),
                        port_context: msg.port_context.clone(),
                    })
                    .await;
            }

            let kept_messages = state
                .actor_thread
                .compact_history_keep_recent(CONTEXT_COMPACTION_KEEP_MESSAGES)
                .await?;
            info!(
                target: "borg_exec",
                actor_id = %state.actor_thread.actor_id,
                kept_messages,
                "context compaction completed; retrying provider call"
            );

            run_result = state
                .actor_thread
                .agent
                .clone()
                .run_with_tool_events(
                    &mut state.actor_thread,
                    &llm,
                    toolchain.as_ref(),
                    tool_event_tx.as_ref(),
                )
                .await;
        }

        drop(tool_event_tx);
        if let Some(task) = tool_event_task {
            let _ = task.await;
        }

        match run_result {
            ActorRunResult::Completed(Ok(output)) => {
                let outbound_messages =
                    parse_outbound_messages_from_llm_reply(&msg.port_context, &output.reply)?;
                Ok(ActorOutput {
                    actor_message_id: actor_message_id.cloned(),
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages,
                    tool_calls: if progress_tx.is_some() {
                        Vec::new()
                    } else {
                        output
                            .tool_calls
                            .iter()
                            .map(|call| ToolCallSummary {
                                tool_name: call.tool_name.clone(),
                                arguments: call.arguments.clone(),
                                output: call.output.clone(),
                            })
                            .collect()
                    },
                    port_context: msg.port_context.clone(),
                })
            }
            ActorRunResult::Completed(Err(err)) => {
                error!("Actor turn completed with error: {}", err);
                Err(anyhow!(err.to_string()))
            }
            ActorRunResult::ActorError(err) => {
                error!("Actor turn error: {}", err);
                Err(anyhow!(err.to_string()))
            }
            ActorRunResult::Idle => Ok(ActorOutput {
                actor_message_id: actor_message_id.cloned(),
                actor_id: state.actor_thread.actor_id.clone(),
                outbound_messages: Vec::new(),
                tool_calls: Vec::new(),
                port_context: msg.port_context.clone(),
            }),
        }
    }

    async fn process_control_command(
        &self,
        state: &mut ActorState,
        msg: &BorgMessage,
        command: &BorgCommand,
    ) -> Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        match command {
            BorgCommand::ModelShowCurrent => {
                let model = self.current_model(&state.actor_id).await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Actor: {}\nModel: {}\nMailbox: {}",
                            self.actor_id, model, state.actor_thread.actor_id
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ModelSet { model } => {
                let model = model.trim();
                if model.is_empty() {
                    return Err(anyhow!("model cannot be empty"));
                }
                self.set_model(&state.actor_id, model).await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Updated model to {} for actor {}.\nMailbox: {}",
                            model, self.actor_id, state.actor_thread.actor_id
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ReasoningShowCurrent => {
                let current = state
                    .current_reasoning_effort
                    .map(|effort| effort.to_string())
                    .unwrap_or_else(|| "default".to_string());
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Current reasoning effort for actor {}: {}",
                            state.actor_thread.actor_id, current
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ReasoningSet { reasoning_effort } => {
                state.current_reasoning_effort = Some(*reasoning_effort);
                state.actor_thread.agent.reasoning_effort = state.current_reasoning_effort;
                self.runtime
                    .db
                    .set_actor_reasoning_effort(
                        &state.actor_thread.actor_id,
                        Some(reasoning_effort.as_str()),
                    )
                    .await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Updated reasoning effort to {} for actor {}.",
                            reasoning_effort, state.actor_thread.actor_id
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ParticipantsList => {
                let participants = self
                    .telegram_participants(&state.actor_thread.actor_id)
                    .await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(&msg.port_context, Some(participants)),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ContextDump => {
                let dump = self.context_dump(&state.actor_thread.actor_id).await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(&msg.port_context, Some(dump)),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::CompactContext => {
                let kept = state
                    .actor_thread
                    .compact_history_keep_recent(CONTEXT_COMPACTION_KEEP_MESSAGES)
                    .await?;
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Compacted mailbox context. Kept {kept} context message(s)."
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ResetContext => {
                let deleted = state.actor_thread.clear_history().await?;
                if let Some((port_name, _)) = self
                    .runtime
                    .db
                    .get_any_port_actor_context(&state.actor_thread.actor_id)
                    .await?
                {
                    let _ = self
                        .runtime
                        .db
                        .clear_port_actor_context(&port_name, &state.actor_thread.actor_id)
                        .await?;
                }
                Ok(ActorOutput {
                    actor_message_id: None,
                    actor_id: state.actor_thread.actor_id.clone(),
                    outbound_messages: make_port_reply_messages(
                        &msg.port_context,
                        Some(format!(
                            "Reset complete. Cleared {} message(s) and port actor context.",
                            deleted
                        )),
                    ),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
        }
    }

    async fn current_model(&self, actor_id: &Uri) -> Result<String> {
        let actor = self
            .runtime
            .db
            .get_actor(actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", actor_id))?;
        actor
            .model
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("model not configured for actor {}", actor_id))
    }

    async fn set_model(&self, actor_id: &Uri, model: &str) -> Result<()> {
        let updated = self.runtime.db.set_actor_model(actor_id, model).await?;
        if updated == 0 {
            return Err(anyhow!("actor not found: {}", actor_id));
        }
        Ok(())
    }

    async fn resolve_execution_actor_id(&self) -> Result<Uri> {
        let _actor = self
            .runtime
            .db
            .get_actor(&self.actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", self.actor_id))?;

        Ok(self.actor_id.clone())
    }

    async fn toolchain_for_turn(
        &self,
        state: &mut ActorState,
        msg: &BorgMessage,
    ) -> Result<Arc<Toolchain<BorgToolCall, BorgToolResult>>> {
        if let Some(cache) = &state.cached_toolchain
            && cache.user_id == msg.user_id
            && cache.actor_id == state.actor_id
            && cache.built_at.elapsed() < TOOLCHAIN_CACHE_TTL
        {
            return Ok(cache.toolchain.clone());
        }

        let toolchain = Arc::new(
            self.runtime
                .build_toolchain(&msg.user_id, &state.actor_id)
                .await?,
        );

        state.cached_toolchain = Some(CachedToolchain {
            user_id: msg.user_id.clone(),
            actor_id: state.actor_id.clone(),
            built_at: Instant::now(),
            toolchain: toolchain.clone(),
        });

        Ok(toolchain)
    }

    async fn telegram_participants(&self, actor_id: &Uri) -> Result<String> {
        let Some(ctx_json) = self
            .runtime
            .db
            .get_port_actor_context("telegram", actor_id)
            .await?
        else {
            return Ok("No Telegram participant context found for this actor.".to_string());
        };
        let ctx: TelegramContext = serde_json::from_value(ctx_json)?;
        if ctx.participants.is_empty() {
            return Ok("No participants tracked in Telegram actor context.".to_string());
        }

        let mut lines = Vec::new();
        lines.push(format!("Chat {} ({})", ctx.chat_id, ctx.chat_type));
        lines.push("Participants:".to_string());
        for participant in ctx.participants.values() {
            let display = participant
                .username
                .as_ref()
                .map(|username| format!("@{username}"))
                .or_else(|| participant.first_name.clone())
                .unwrap_or_else(|| participant.id.clone());
            lines.push(format!("- {} [{}]", display, participant.id));
        }
        Ok(lines.join("\n"))
    }

    async fn context_dump(&self, actor_id: &Uri) -> Result<String> {
        let total = self.runtime.db.count_actor_messages(actor_id).await?;
        let limit = 20_usize;
        let from = total.saturating_sub(limit);
        let messages = self
            .runtime
            .db
            .list_actor_messages(actor_id, from, limit)
            .await?;
        Ok(serde_json::to_string_pretty(&messages)?)
    }

    async fn load_actor_reasoning_effort(&self, actor_id: &Uri) -> Result<Option<ReasoningEffort>> {
        Ok(self
            .runtime
            .db
            .get_actor_reasoning_effort(actor_id)
            .await?
            .as_deref()
            .and_then(|value| ReasoningEffort::from_str(value).ok()))
    }
}

fn is_context_window_overflow_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("maximum context length")
        || normalized.contains("context length")
        || normalized.contains("too many tokens")
        || normalized.contains("reduce the length")
        || normalized.contains("context window")
}

#[derive(serde::Deserialize)]
struct NdjsonPortReply {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default, rename = "replyTo")]
    reply_to_camel: Option<String>,
    #[serde(default)]
    port_context: Option<PortContext>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn parse_outbound_messages_from_llm_reply(
    default_port_context: &PortContext,
    reply: &str,
) -> Result<Vec<ActorOutboundMessage>> {
    if matches!(default_port_context, PortContext::Unknown) {
        return Ok(Vec::new());
    }

    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    // Try single JSON object first (e.g. {"kind":"port_reply","text":"hello"})
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let action: NdjsonPortReply = serde_json::from_str(trimmed).map_err(|err| {
            anyhow!("invalid JSON reply (expected port_reply object): {err}")
        })?;

        if let Some(kind) = &action.kind {
            if kind.trim().to_ascii_lowercase() != "port_reply" {
                return Err(anyhow!("unsupported outbound action kind `{kind}`"));
            }
        }

        let text = action
            .text
            .or(action.content)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("port_reply requires non-empty `text` or `content`"))?;

        return Ok(vec![ActorOutboundMessage::PortReply {
            text,
            port_context: action.port_context.unwrap_or_else(|| default_port_context.clone()),
            metadata: action.metadata.unwrap_or_else(|| json!({})),
        }]);
    }

    // Try NDJSON (multiple lines, each is a JSON object)
    let mut outbound_messages = Vec::new();
    for line in trimmed.lines() {
        let row = line.trim();
        if row.is_empty() {
            continue;
        }
        if !row.starts_with('{') || !row.ends_with('}') {
            return Err(anyhow!("invalid NDJSON reply line (expected JSON object): {row}"));
        }

        let action: NdjsonPortReply = serde_json::from_str(row).map_err(|err| {
            anyhow!("invalid NDJSON reply line (expected JSON object): {err}")
        })?;

        if let Some(kind) = &action.kind {
            if kind.trim().to_ascii_lowercase() != "port_reply" {
                return Err(anyhow!("unsupported outbound action kind `{kind}`"));
            }
        }

        let text = action
            .text
            .or(action.content)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("port_reply requires non-empty text or content"))?;

        outbound_messages.push(ActorOutboundMessage::PortReply {
            text,
            port_context: action.port_context.unwrap_or_else(|| default_port_context.clone()),
            metadata: action.metadata.unwrap_or_else(|| json!({})),
        });
    }

    if outbound_messages.is_empty() {
        return Err(anyhow!("no valid port_reply objects found in response"));
    }

    Ok(outbound_messages)
}

fn make_port_reply_messages(
    port_context: &PortContext,
    reply: Option<String>,
) -> Vec<ActorOutboundMessage> {
    let text = reply
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    match text {
        Some(text) => vec![ActorOutboundMessage::PortReply {
            text,
            port_context: port_context.clone(),
            metadata: json!({}),
        }],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::is_context_window_overflow_error;

    #[test]
    fn context_overflow_detection_matches_provider_message() {
        let err = "provider `openrouter` chat_completion response error: status 400 Bad Request: This endpoint's maximum context length is 1048576 tokens.";
        assert!(is_context_window_overflow_error(err));
    }

    #[test]
    fn context_overflow_detection_ignores_unrelated_errors() {
        let err = "provider `openrouter` chat_completion response error: status 401 Unauthorized";
        assert!(!is_context_window_overflow_error(err));
    }
}
