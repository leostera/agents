use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use borg_agent::{
    ActorThread, Agent, BorgToolCall, BorgToolResult, BorgToolResult as BorgToolResultInner,
    Message as AgentMessage, ToolOutputEnvelope,
};
use borg_core::{ActorId, MessagePayload, WorkspaceId};
use borg_db::BorgDb;

use crate::mailbox::ActorCommand;
use crate::runtime::BorgRuntime;

#[derive(Clone)]
pub struct ActorHandle {
    pub actor_id: ActorId,
    pub mailbox: mpsc::Sender<ActorCommand>,
    pub task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ActorHandle {
    pub async fn abort(&self) {
        if let Some(handle) = self.task.lock().await.take() {
            handle.abort();
        }
    }
}

pub struct Actor {
    pub actor_id: ActorId,
    pub workspace_id: WorkspaceId,
    pub db: BorgDb,
    pub runtime: Arc<BorgRuntime>,
    pub agent: Agent<BorgToolCall, BorgToolResult>,
    pub thread: ActorThread<BorgToolCall, BorgToolResult>,
}

impl Actor {
    pub async fn spawn(
        actor_id: ActorId,
        workspace_id: WorkspaceId,
        runtime: Arc<BorgRuntime>,
    ) -> Result<ActorHandle> {
        let (tx, rx) = mpsc::channel(100);
        let actor_id_clone = actor_id.clone();
        let db = runtime.db.clone();

        let actor = Self::new(actor_id.clone(), workspace_id, db, runtime).await?;

        let task_handle = Arc::new(Mutex::new(None));
        let task_handle_inner = task_handle.clone();

        let task = tokio::spawn(async move {
            if let Err(err) = actor.run(rx).await {
                error!(actor_id = %actor_id_clone, error = %err, "actor failed");
            }
        });

        *task_handle_inner.lock().await = Some(task);

        Ok(ActorHandle {
            actor_id,
            mailbox: tx,
            task: task_handle,
        })
    }

    async fn new(
        actor_id: ActorId,
        workspace_id: WorkspaceId,
        db: BorgDb,
        runtime: Arc<BorgRuntime>,
    ) -> Result<Self> {
        let agent = runtime
            .actor_context_manager
            .resolve_agent_for_turn(&actor_id)
            .await?;
        let mut thread = ActorThread::new(
            actor_id.clone(),
            workspace_id.clone(),
            agent.clone(),
            db.clone(),
        )
        .await?;

        let context_manager = runtime
            .actor_context_manager
            .build_context_manager(&actor_id)
            .await?;
        thread.set_context_manager(context_manager);

        Ok(Self {
            actor_id,
            workspace_id,
            db,
            runtime,
            agent,
            thread,
        })
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<ActorCommand>) -> Result<()> {
        debug!(actor_id = %self.actor_id, "actor loop started");

        // 1. Replay durable mailbox / Initial history load
        self.load_history().await?;

        // 2. Main loop
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ActorCommand::Terminate => {
                    info!(actor_id = %self.actor_id, "terminating actor");
                    return Ok(());
                }
                ActorCommand::Message(message_record) => {
                    self.process_message(message_record).await?;
                }
                ActorCommand::Notify => {
                    while let Some(msg) = self
                        .db
                        .claim_next_pending_message(&self.actor_id.clone().into())
                        .await?
                    {
                        self.process_message(msg).await?;
                    }
                }
                ActorCommand::InspectContext(tx) => {
                    let _ = tx.send(self.thread.build_context().await);
                }
            }
        }
        Ok(())
    }

    async fn load_history(&mut self) -> Result<()> {
        let records = self
            .db
            .list_messages(&self.actor_id.clone().into(), 100)
            .await?;
        for record in records {
            let agent_msg = self.map_message(record)?;
            if let Some(msg) = agent_msg {
                self.thread.add_message(msg).await?;
            }
        }
        Ok(())
    }

    fn map_message(
        &self,
        msg: borg_db::MessageRecord,
    ) -> Result<Option<AgentMessage<BorgToolCall, BorgToolResult>>> {
        let agent_msg = match &msg.payload {
            MessagePayload::UserText(p) => Some(AgentMessage::User {
                content: p.text.clone(),
            }),
            MessagePayload::UserAudio(p) => Some(AgentMessage::UserAudio {
                file_id: borg_core::Uri::parse(&p.file_id)?,
                transcript: p.transcript.clone().unwrap_or_default(),
                created_at: msg.delivered_at.to_rfc3339(),
            }),
            MessagePayload::AssistantText(p) => Some(AgentMessage::Assistant {
                content: p.text.clone(),
            }),
            MessagePayload::FinalAssistantMessage(p) => Some(AgentMessage::Assistant {
                content: p.text.clone(),
            }),
            MessagePayload::ToolCall(p) => Some(AgentMessage::ToolCall {
                tool_call_id: p.tool_call_id.to_string(),
                name: p.tool_name.clone(),
                arguments: serde_json::from_str(&p.arguments_json).unwrap_or_default(),
            }),
            MessagePayload::ToolResult(p) => Some(AgentMessage::ToolResult {
                tool_call_id: p.tool_call_id.to_string(),
                name: p.tool_name.clone(),
                content: if p.is_error {
                    ToolOutputEnvelope::Error(p.result_json.clone())
                } else {
                    ToolOutputEnvelope::Ok(BorgToolResultInner::from(
                        serde_json::from_str::<serde_json::Value>(&p.result_json)
                            .unwrap_or_default(),
                    ))
                },
            }),
            _ => None,
        };
        Ok(agent_msg)
    }

    async fn process_message(&mut self, msg: borg_db::MessageRecord) -> Result<()> {
        info!(actor_id = %self.actor_id, message_id = %msg.message_id, "processing message");

        let agent_msg = self.map_message(msg.clone())?;
        if let Some(m) = agent_msg {
            self.thread.add_message(m).await?;
        } else {
            debug!("skipping non-compatible message: {}", msg.payload.kind());
            return Ok(());
        }

        let llm = self.runtime.llm().await?;
        let toolchain = self
            .runtime
            .build_toolchain(&msg.sender_id, &self.actor_id)
            .await?;

        let result = self
            .agent
            .run(&mut self.thread, llm.as_ref(), &toolchain)
            .await;

        match result {
            borg_agent::ActorRunResult::Completed(Ok(output)) => {
                let reply_payload = MessagePayload::final_assistant(output.reply);
                self.runtime
                    .send_message(&self.actor_id.clone().into(), &msg.sender_id, reply_payload)
                    .await?;

                self.db.mark_message_processed(&msg.message_id).await?;
            }
            borg_agent::ActorRunResult::Completed(Err(err))
            | borg_agent::ActorRunResult::ActorError(err) => {
                error!(actor_id = %self.actor_id, message_id = %msg.message_id, error = %err, "turn failed");
                self.db
                    .mark_message_failed(&msg.message_id, "agent_error", &err)
                    .await?;
            }
            borg_agent::ActorRunResult::Idle => {
                self.db.mark_message_processed(&msg.message_id).await?;
            }
        }

        Ok(())
    }
}
