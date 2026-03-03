use anyhow::{Result, anyhow};
use borg_agent::{Message, Session, SessionResult};
use borg_core::Uri;
use borg_llm::TranscriptionRequest;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::mailbox::ActorCommand;
use crate::message::{BorgCommand, BorgInput, BorgMessage, SessionOutput, ToolCallSummary};
use crate::port_context::TelegramSessionContext;
use crate::runtime::BorgRuntime;

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

struct SessionState {
    session: Session<Value, Value>,
    agent_id: Uri,
    behavior_id: Option<Uri>,
}

struct Actor {
    actor_id: Uri,
    mailbox: mpsc::Sender<ActorCommand>,
    rx: mpsc::Receiver<ActorCommand>,
    runtime: Arc<BorgRuntime>,
    sessions: HashMap<Uri, SessionState>,
}

impl Actor {
    async fn new(actor_id: Uri, runtime: Arc<BorgRuntime>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        Ok(Self {
            actor_id,
            mailbox: tx,
            rx,
            runtime,
            sessions: HashMap::new(),
        })
    }

    fn mailbox(&self) -> mpsc::Sender<ActorCommand> {
        self.mailbox.clone()
    }

    async fn run(mut self) {
        debug!("actor {} loop started", self.actor_id);
        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    match cmd {
                        ActorCommand::Cast { actor_message_id, msg } => {
                            let result = self.process_message(msg).await;
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
                            msg,
                            response_tx,
                        } => {
                            let result = self.process_message(msg).await;
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

    async fn process_message(&mut self, msg: BorgMessage) -> Result<SessionOutput<Value, Value>> {
        if !self.sessions.contains_key(&msg.session_id) {
            let (agent_id, behavior_id) = self.resolve_execution_agent_id().await?;
            let session = self.create_session(&msg.session_id, &agent_id).await?;
            self.sessions.insert(
                msg.session_id.clone(),
                SessionState {
                    session,
                    agent_id,
                    behavior_id,
                },
            );
        }

        let drop_after = matches!(&msg.input, BorgInput::Command(BorgCommand::ResetContext));
        let mut state = self
            .sessions
            .remove(&msg.session_id)
            .ok_or_else(|| anyhow!("session state missing for {}", msg.session_id))?;

        let result = match &msg.input {
            BorgInput::Chat { text } => self.process_chat_message(&mut state, &msg, text).await,
            BorgInput::Audio {
                file_id,
                mime_type,
                duration_ms,
                language_hint,
            } => {
                self.process_audio_message(
                    &mut state,
                    &msg,
                    file_id,
                    mime_type.as_deref(),
                    *duration_ms,
                    language_hint.as_deref(),
                )
                .await
            }
            BorgInput::Command(command) => {
                self.process_control_command(&mut state, &msg, command)
                    .await
            }
        };

        if !drop_after {
            self.sessions.insert(msg.session_id.clone(), state);
        }

        result
    }

    async fn create_session(
        &self,
        session_id: &Uri,
        agent_id: &Uri,
    ) -> Result<Session<Value, Value>> {
        self.runtime
            .session_manager
            .session_for_task(Some(session_id.clone()), Some(agent_id))
            .await
    }

    async fn process_chat_message(
        &self,
        state: &mut SessionState,
        msg: &BorgMessage,
        text: &str,
    ) -> Result<SessionOutput<Value, Value>> {
        state
            .session
            .add_message(Message::User {
                content: text.to_string(),
            })
            .await?;

        self.process_text_turn(state, msg).await
    }

    async fn process_audio_message(
        &self,
        state: &mut SessionState,
        msg: &BorgMessage,
        file_id: &Uri,
        mime_type_hint: Option<&str>,
        _duration_ms: Option<u64>,
        language_hint: Option<&str>,
    ) -> Result<SessionOutput<Value, Value>> {
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
            .session
            .add_message(Message::UserAudio {
                file_id: file_id.clone(),
                transcript: transcript.clone(),
                created_at: Utc::now().to_rfc3339(),
            })
            .await?;

        self.process_text_turn(state, msg).await
    }

    async fn process_text_turn(
        &self,
        state: &mut SessionState,
        msg: &BorgMessage,
    ) -> Result<SessionOutput<Value, Value>> {
        let (agent_id, behavior_id) = self.resolve_execution_agent_id().await?;
        state.agent_id = agent_id;
        state.behavior_id = behavior_id;
        state.session.agent = self
            .runtime
            .session_manager
            .resolve_agent_for_turn(&state.agent_id, state.behavior_id.as_ref())
            .await?;

        let llm = self.runtime.llm().await?;
        let toolchain = self
            .runtime
            .build_toolchain(&msg.user_id, &msg.session_id, &state.agent_id)
            .await?;
        match state
            .session
            .agent
            .clone()
            .run(&mut state.session, &llm, &toolchain)
            .await
        {
            SessionResult::Completed(Ok(output)) => Ok(SessionOutput {
                session_id: state.session.session_id.clone(),
                reply: Some(output.reply),
                tool_calls: output
                    .tool_calls
                    .iter()
                    .map(|call| ToolCallSummary {
                        tool_name: call.tool_name.clone(),
                        arguments: call.arguments.clone(),
                        output: call.output.clone(),
                    })
                    .collect(),
                port_context: msg.port_context.clone(),
            }),
            SessionResult::Completed(Err(err)) => {
                error!("Session completed with error: {}", err);
                Err(anyhow!(err.to_string()))
            }
            SessionResult::SessionError(err) => {
                error!("Session error: {}", err);
                Err(anyhow!(err.to_string()))
            }
            SessionResult::Idle => Ok(SessionOutput {
                session_id: state.session.session_id.clone(),
                reply: None,
                tool_calls: Vec::new(),
                port_context: msg.port_context.clone(),
            }),
        }
    }

    async fn process_control_command(
        &self,
        state: &mut SessionState,
        msg: &BorgMessage,
        command: &BorgCommand,
    ) -> Result<SessionOutput<Value, Value>> {
        match command {
            BorgCommand::ModelShowCurrent => {
                let model = self.current_model(&state.agent_id).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(format!(
                        "Actor: {}\nBehavior: {}\nModel: {}\nSession: {}",
                        self.actor_id,
                        state
                            .behavior_id
                            .as_ref()
                            .map(Uri::as_str)
                            .unwrap_or("unknown"),
                        model,
                        msg.session_id
                    )),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ModelSet { model } => {
                let model = model.trim();
                if model.is_empty() {
                    return Err(anyhow!("model cannot be empty"));
                }
                self.set_model(&state.agent_id, model).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(format!(
                        "Updated model to {} for actor {}.\nSession: {}",
                        model, self.actor_id, msg.session_id
                    )),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ParticipantsList => {
                let participants = self.telegram_participants(&msg.session_id).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(participants),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ContextDump => {
                let dump = self.context_dump(&msg.session_id).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(dump),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::CompactSession => {
                let kept = self.compact_session(&msg.session_id).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(format!(
                        "Compacted session. Kept {kept} context message(s)."
                    )),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ResetContext => {
                let deleted = self
                    .runtime
                    .db
                    .clear_session_history(&msg.session_id)
                    .await?;
                if let Some((port_name, _)) = self
                    .runtime
                    .db
                    .get_any_port_session_context(&msg.session_id)
                    .await?
                {
                    let _ = self
                        .runtime
                        .db
                        .clear_port_session_context(&port_name, &msg.session_id)
                        .await?;
                }
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(format!(
                        "Reset complete. Cleared {} message(s) and port session context.",
                        deleted
                    )),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
        }
    }

    async fn current_model(&self, agent_id: &Uri) -> Result<String> {
        let maybe = self.runtime.db.get_agent_spec(agent_id).await?;
        Ok(maybe
            .map(|spec| spec.model)
            .unwrap_or_else(|| "unknown".to_string()))
    }

    async fn set_model(&self, agent_id: &Uri, model: &str) -> Result<()> {
        let existing = self.runtime.db.get_agent_spec(agent_id).await?;
        let (name, default_provider_id, system_prompt) = if let Some(spec) = existing {
            (spec.name, spec.default_provider_id, spec.system_prompt)
        } else {
            (fallback_agent_name(agent_id), None, String::new())
        };
        self.runtime
            .db
            .upsert_agent_spec(
                agent_id,
                &name,
                default_provider_id.as_deref(),
                model,
                &system_prompt,
            )
            .await
    }

    async fn resolve_execution_agent_id(&self) -> Result<(Uri, Option<Uri>)> {
        let actor = self
            .runtime
            .db
            .get_actor(&self.actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", self.actor_id))?;
        let behavior = self
            .runtime
            .db
            .get_behavior(&actor.default_behavior_id)
            .await?;

        let agent_id = self.actor_id.clone();
        let existing = self.runtime.db.get_agent_spec(&agent_id).await?;
        let model = if let Some(spec) = existing {
            spec.model
        } else {
            self.resolve_behavior_default_model(
                behavior
                    .as_ref()
                    .and_then(|record| record.preferred_provider_id.as_deref()),
            )
            .await
        };

        let system_prompt = behavior
            .as_ref()
            .map(|record| record.system_prompt.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(actor.system_prompt.as_str());
        let default_provider_id = behavior
            .as_ref()
            .and_then(|record| record.preferred_provider_id.as_deref());
        let agent_name = actor.name.clone();

        self.runtime
            .db
            .upsert_agent_spec(
                &agent_id,
                &agent_name,
                default_provider_id,
                &model,
                system_prompt,
            )
            .await?;

        Ok((agent_id, Some(actor.default_behavior_id)))
    }

    async fn resolve_behavior_default_model(&self, preferred_provider_id: Option<&str>) -> String {
        if let Some(provider_id) = preferred_provider_id
            && let Ok(Some(provider)) = self.runtime.db.get_provider(provider_id).await
            && provider.enabled
            && let Some(model) = provider.default_text_model
            && !model.trim().is_empty()
        {
            return model;
        }

        if let Ok(providers) = self.runtime.db.list_providers(128).await {
            for provider in providers {
                if !provider.enabled {
                    continue;
                }
                if let Some(model) = provider.default_text_model
                    && !model.trim().is_empty()
                {
                    return model;
                }
            }
        }

        "gpt-4o-mini".to_string()
    }

    async fn telegram_participants(&self, session_id: &Uri) -> Result<String> {
        let Some(ctx_json) = self
            .runtime
            .db
            .get_port_session_context("telegram", session_id)
            .await?
        else {
            return Ok("No Telegram participant context found for this session.".to_string());
        };
        let ctx = TelegramSessionContext::from_json(ctx_json)?;
        if ctx.participants.is_empty() {
            return Ok("No participants tracked in Telegram session context.".to_string());
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

    async fn context_dump(&self, session_id: &Uri) -> Result<String> {
        let total = self.runtime.db.count_session_messages(session_id).await?;
        let limit = 20_usize;
        let from = total.saturating_sub(limit);
        let messages = self
            .runtime
            .db
            .list_session_messages(session_id, from, limit)
            .await?;
        Ok(serde_json::to_string_pretty(&messages)?)
    }

    async fn compact_session(&self, session_id: &Uri) -> Result<usize> {
        const KEEP_MESSAGES: usize = 24;

        let total = self.runtime.db.count_session_messages(session_id).await?;
        if total <= KEEP_MESSAGES {
            return Ok(total);
        }

        let from = total.saturating_sub(KEEP_MESSAGES);
        let tail = self
            .runtime
            .db
            .list_session_messages(session_id, from, KEEP_MESSAGES)
            .await?;
        self.runtime.db.clear_session_history(session_id).await?;
        for payload in &tail {
            self.runtime
                .db
                .append_session_message(session_id, payload)
                .await?;
        }
        Ok(tail.len())
    }
}

fn fallback_agent_name(agent_id: &Uri) -> String {
    let raw = agent_id.as_str();
    if raw == "borg:agent:default" {
        return "Default Agent".to_string();
    }
    raw.rsplit(':')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("Agent")
        .to_string()
}
