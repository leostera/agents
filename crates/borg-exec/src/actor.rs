use anyhow::{Result, anyhow};
use borg_agent::{
    AgentTools, Message, Session, SessionContextManager, SessionEventPayload, SessionResult,
};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::mailbox::ActorCommand;
use crate::message::{BorgCommand, BorgInput, BorgMessage, SessionOutput, ToolCallSummary};
use crate::port_context::TelegramSessionContext;
use crate::runtime::BorgRuntime;
use crate::types::UserMessage;

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

    async fn resolve_agent_id(db: &BorgDb, session_id: &Uri) -> Result<Uri> {
        let messages = db.list_session_messages(session_id, 0, 64).await?;
        for message in messages {
            let Ok(message) = serde_json::from_value::<Message>(message) else {
                continue;
            };
            if let Message::SessionEvent {
                payload: SessionEventPayload::Started { agent_id },
                ..
            } = message
            {
                return Ok(agent_id);
            }
        }

        let specs = db.list_agent_specs(1).await?;
        if let Some(first) = specs.into_iter().next() {
            return Ok(first.agent_id);
        }

        Ok(uri!("borg", "agent", "default"))
    }
}

struct SessionState {
    session: Session,
    agent_id: Uri,
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

    async fn process_message(&mut self, msg: BorgMessage) -> Result<SessionOutput> {
        if !self.sessions.contains_key(&msg.session_id) {
            let agent_id = ActorHandle::resolve_agent_id(&self.runtime.db, &msg.session_id).await?;
            let session = self.create_session(&msg.session_id, &agent_id).await?;
            self.sessions
                .insert(msg.session_id.clone(), SessionState { session, agent_id });
        }

        let drop_after = matches!(&msg.input, BorgInput::Command(BorgCommand::ResetContext));
        let mut state = self
            .sessions
            .remove(&msg.session_id)
            .ok_or_else(|| anyhow!("session state missing for {}", msg.session_id))?;

        let result = match &msg.input {
            BorgInput::Chat { text } => self.process_chat_message(&mut state, &msg, text).await,
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

    async fn create_session(&self, session_id: &Uri, agent_id: &Uri) -> Result<Session> {
        let synthetic_msg = UserMessage {
            user_id: Uri::from_parts("borg", "user", Some("system"))?,
            text: String::new(),
            session_id: Some(session_id.clone()),
            agent_id: Some(agent_id.clone()),
            metadata: serde_json::json!({}),
        };

        let mut session = self
            .runtime
            .session_manager
            .session_for_task(&synthetic_msg)
            .await?;
        if let Some((port, ctx)) = self
            .runtime
            .db
            .get_any_port_session_context(session_id)
            .await?
            && port == "telegram"
        {
            session.set_context_manager(Arc::new(
                SessionContextManager::for_telegram_session_context(ctx),
            ));
        }
        Ok(session)
    }

    async fn process_chat_message(
        &self,
        state: &mut SessionState,
        msg: &BorgMessage,
        text: &str,
    ) -> Result<SessionOutput> {
        let user_msg = UserMessage {
            user_id: msg.user_id.clone(),
            text: text.to_string(),
            session_id: Some(msg.session_id.clone()),
            agent_id: Some(state.agent_id.clone()),
            metadata: serde_json::json!({}),
        };

        state
            .session
            .add_message(Message::User {
                content: text.to_string(),
            })
            .await?;

        let llm = self.runtime.llm().await?;
        let toolchain = self
            .runtime
            .build_toolchain(&user_msg, &msg.session_id, &state.agent_id)
            .await?;
        let tools = AgentTools {
            tool_runner: &toolchain,
        };

        match state
            .session
            .agent
            .clone()
            .run(&mut state.session, &llm, &tools)
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
                        output: serde_json::to_value(&call.output)
                            .unwrap_or_else(|_| serde_json::json!({})),
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
    ) -> Result<SessionOutput> {
        match command {
            BorgCommand::ModelShowCurrent => {
                let model = self.current_model(&state.agent_id).await?;
                Ok(SessionOutput {
                    session_id: msg.session_id.clone(),
                    reply: Some(format!(
                        "Agent: {}\nModel: {}\nSession: {}",
                        state.agent_id, model, msg.session_id
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
                        "Updated model to {} for agent {}.\nSession: {}",
                        model, state.agent_id, msg.session_id
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
