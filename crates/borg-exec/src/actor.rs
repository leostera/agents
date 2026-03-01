use anyhow::{Result, anyhow};
use borg_agent::{
    AgentTools, Message, Session, SessionContextManager, SessionEventPayload, SessionResult,
};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::mailbox::ActorCommand;
use crate::message::{BorgCommand, BorgInput, BorgMessage, SessionOutput, ToolCallSummary};
use crate::runtime::BorgRuntime;
use crate::types::UserMessage;

pub struct ActorHandle {
    pub session_id: Uri,
    pub agent_id: Uri,
    pub mailbox: mpsc::Sender<ActorCommand>,
    pub task: Arc<JoinHandle<()>>,
}

impl Clone for ActorHandle {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            agent_id: self.agent_id.clone(),
            mailbox: self.mailbox.clone(),
            task: Arc::clone(&self.task),
        }
    }
}

impl ActorHandle {
    pub async fn spawn(session_id: Uri, runtime: Arc<BorgRuntime>) -> Result<Self> {
        let db = runtime.db.clone();
        let agent_id = Self::resolve_agent_id(&db, &session_id).await?;

        let actor = Actor::new(session_id.clone(), agent_id.clone(), runtime).await?;
        let mailbox = actor.mailbox();
        let session_id = actor.session_id.clone();

        let task = Arc::new(tokio::spawn(async move {
            actor.run().await;
        }));

        Ok(Self {
            session_id,
            agent_id,
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

struct Actor {
    session_id: Uri,
    agent_id: Uri,
    mailbox: mpsc::Sender<ActorCommand>,
    rx: mpsc::Receiver<ActorCommand>,
    runtime: Arc<BorgRuntime>,
}

impl Actor {
    async fn new(session_id: Uri, agent_id: Uri, runtime: Arc<BorgRuntime>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        Ok(Self {
            session_id,
            agent_id,
            mailbox: tx,
            rx,
            runtime,
        })
    }

    fn mailbox(&self) -> mpsc::Sender<ActorCommand> {
        self.mailbox.clone()
    }

    async fn run(mut self) {
        debug!("actor {} loop started", self.session_id);

        let session = match self.create_session().await {
            Ok(s) => s,
            Err(e) => {
                error!("actor {} failed to create session: {}", self.session_id, e);
                return;
            }
        };
        let mut session = session;

        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    match cmd {
                        ActorCommand::Cast(msg) => {
                            let _ = self.process_message(&mut session, msg).await;
                        }
                        ActorCommand::Call(msg, response_tx) => {
                            let result = self.process_message(&mut session, msg).await;
                            let _ = response_tx.send(result);
                        }
                        ActorCommand::Terminate => {
                            info!("actor {} terminating", self.session_id);
                            break;
                        }
                    }
                }
                else => break,
            }
        }

        debug!("actor {} loop ended", self.session_id);
    }

    async fn create_session(&self) -> Result<Session> {
        let synthetic_msg = UserMessage {
            user_id: Uri::from_parts("borg", "user", Some("system"))?,
            text: String::new(),
            session_id: Some(self.session_id.clone()),
            agent_id: Some(self.agent_id.clone()),
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
            .get_any_port_session_context(&self.session_id)
            .await?
            && port == "telegram"
        {
            session.set_context_manager(Arc::new(
                SessionContextManager::for_telegram_session_context(ctx),
            ));
        }
        Ok(session)
    }

    async fn process_message(
        &mut self,
        session: &mut Session,
        msg: BorgMessage,
    ) -> Result<SessionOutput> {
        match &msg.input {
            BorgInput::Chat { text } => self.process_chat_message(session, &msg, text).await,
            BorgInput::Command(command) => self.process_control_command(&msg, command).await,
        }
    }

    async fn process_chat_message(
        &mut self,
        session: &mut Session,
        msg: &BorgMessage,
        text: &str,
    ) -> Result<SessionOutput> {
        let user_msg = UserMessage {
            user_id: msg.user_id.clone(),
            text: text.to_string(),
            session_id: Some(self.session_id.clone()),
            agent_id: Some(self.agent_id.clone()),
            metadata: serde_json::json!({}),
        };

        session
            .add_message(Message::User {
                content: text.to_string(),
            })
            .await?;

        let llm = self.runtime.llm().await?;
        let toolchain = self
            .runtime
            .build_toolchain(&user_msg, &self.session_id)
            .await?;
        let tools = AgentTools {
            tool_runner: &toolchain,
        };

        match session.agent.clone().run(session, &llm, &tools).await {
            SessionResult::Completed(Ok(output)) => Ok(SessionOutput {
                session_id: session.session_id.clone(),
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
                session_id: session.session_id.clone(),
                reply: None,
                tool_calls: Vec::new(),
                port_context: msg.port_context.clone(),
            }),
        }
    }

    async fn process_control_command(
        &mut self,
        msg: &BorgMessage,
        command: &BorgCommand,
    ) -> Result<SessionOutput> {
        match command {
            BorgCommand::CompactSession => {
                // Placeholder for upcoming borg-cmd reintegration.
                Ok(SessionOutput {
                    session_id: self.session_id.clone(),
                    reply: Some("compact is not yet wired in runtime".to_string()),
                    tool_calls: Vec::new(),
                    port_context: msg.port_context.clone(),
                })
            }
            BorgCommand::ResetContext => {
                let deleted = self
                    .runtime
                    .db
                    .clear_session_history(&self.session_id)
                    .await?;
                if let Some((port_name, _)) = self
                    .runtime
                    .db
                    .get_any_port_session_context(&self.session_id)
                    .await?
                {
                    let _ = self
                        .runtime
                        .db
                        .clear_port_session_context(&port_name, &self.session_id)
                        .await?;
                }

                Ok(SessionOutput {
                    session_id: self.session_id.clone(),
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
}
