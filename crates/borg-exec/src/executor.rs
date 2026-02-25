use anyhow::{Result, anyhow};
use borg_agent::{AgentTools, Message, SessionResult};
use borg_core::{Event, Task, TaskKind, TaskStatus, Uri};
use borg_db::{BorgDb, NewTask};
use borg_llm::providers::configured::{ConfiguredProvider, ProviderSettings};
use borg_rt::RuntimeEngine;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::session_manager::SessionManager;
use crate::task_queue::TaskQueue;
use crate::tool_runner::{ExecToolRunner, search_capabilities};
use crate::types::InboxMessage;

const OPENAI_PROVIDER: &str = "openai";
const QUEUED_RETRY_DELAY_MILLIS: u64 = 100;
const DEFAULT_AGENT_MODEL: &str = "gpt-4o-mini";

#[derive(Clone)]
pub struct BorgExecutor {
    db: BorgDb,
    runtime: RuntimeEngine,
    worker_id: Uri,
    task_queue: TaskQueue,
    session_manager: SessionManager,
    openai_base_url: Option<String>,
    agent_model: String,
}

pub type ExecEngine = BorgExecutor;

impl BorgExecutor {
    pub fn new(db: BorgDb, runtime: RuntimeEngine, worker_id: Uri) -> Self {
        let task_queue = TaskQueue::new();
        let agent_model = DEFAULT_AGENT_MODEL.to_string();
        let session_manager = SessionManager::new(db.clone(), agent_model.clone());
        Self {
            db,
            runtime,
            worker_id,
            task_queue,
            session_manager,
            openai_base_url: None,
            agent_model,
        }
    }

    pub fn with_openai_base_url(mut self, base_url: Option<String>) -> Self {
        self.openai_base_url = base_url;
        self
    }

    pub fn with_agent_model(mut self, model: impl Into<String>) -> Self {
        self.agent_model = model.into();
        self.session_manager = SessionManager::new(self.db.clone(), self.agent_model.clone());
        self
    }

    async fn configured_provider(&self) -> Result<ConfiguredProvider> {
        let settings = ProviderSettings {
            openai_api_key: self.db.get_provider_api_key(OPENAI_PROVIDER).await?,
            openai_base_url: self.openai_base_url.clone(),
            preferred_provider: None,
        };
        ConfiguredProvider::from_settings(settings)
    }

    pub async fn enqueue_user_message(
        &self,
        mut msg: InboxMessage,
        requested_session_id: Option<String>,
    ) -> Result<(Uri, String)> {
        let session_id =
            requested_session_id.unwrap_or_else(|| format!("borg:session:{}", Uuid::now_v7()));
        msg.session_id = Some(session_id.clone());
        let payload = serde_json::to_value(msg)?;
        let task_id = self
            .db
            .enqueue_task(NewTask {
                kind: TaskKind::UserMessage,
                payload,
                parent_task_id: None,
            })
            .await?;
        self.task_queue.queue(task_id.clone()).await?;
        Ok((task_id, session_id))
    }

    pub async fn queue_task_id(&self, task_id: Uri) -> Result<()> {
        self.task_queue.queue(task_id).await
    }

    pub async fn run(self) -> Result<()> {
        self.recover_tasks_on_startup().await?;
        info!(
            target: "borg_exec",
            worker_id = %self.worker_id,
            "executor loop started"
        );

        loop {
            let task_id = self.task_queue.next().await?;
            if let Err(err) = self.process_task_id(&task_id).await {
                error!(
                    target: "borg_exec",
                    task_id = %task_id,
                    error = %err,
                    "executor task processing failed"
                );
            }
        }
    }

    async fn recover_tasks_on_startup(&self) -> Result<()> {
        let requeued = self.db.requeue_running_tasks().await?;
        if requeued > 0 {
            info!(
                target: "borg_exec",
                requeued,
                "requeued running tasks at startup"
            );
        }

        let recoverable = self.db.list_recoverable_task_ids().await?;
        info!(
            target: "borg_exec",
            queued = recoverable.len(),
            "queueing recoverable tasks at startup"
        );
        for task_id in recoverable {
            self.task_queue.queue(task_id).await?;
        }
        Ok(())
    }

    async fn process_task_id(&self, task_id: &Uri) -> Result<()> {
        let Some(task) = self.db.claim_task_by_id(&self.worker_id, task_id).await? else {
            if let Some(task) = self.db.get_task(task_id).await? {
                if task.status == TaskStatus::Queued {
                    debug!(
                        target: "borg_exec",
                        task_id = %task_id,
                        delay_ms = QUEUED_RETRY_DELAY_MILLIS,
                        "task still queued but not claimable yet, requeueing"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(QUEUED_RETRY_DELAY_MILLIS))
                        .await;
                    self.task_queue.queue(task_id.clone()).await?;
                }
            }
            debug!(
                target: "borg_exec",
                task_id = %task_id,
                "task was not claimable when popped from queue"
            );
            return Ok(());
        };

        info!(
            target: "borg_exec",
            task_id = %task.task_id,
            kind = task.kind.as_str(),
            "claimed task for execution"
        );
        self.db
            .log_event(Event::TaskClaimed {
                task_id: task.task_id.clone(),
                worker_id: self.worker_id.clone(),
            })
            .await?;

        match self.process_task(task.clone()).await {
            Ok(()) => {
                info!(
                    target: "borg_exec",
                    task_id = %task.task_id,
                    "task execution completed successfully"
                );
            }
            Err(err) => {
                error!(
                    target: "borg_exec",
                    task_id = %task.task_id,
                    error = %err,
                    "task execution failed"
                );
                self.db
                    .fail_task(&task.task_id, &err.to_string())
                    .await?;
            }
        }

        Ok(())
    }

    async fn process_task(&self, task: Task) -> Result<()> {
        match task.kind {
            TaskKind::UserMessage => self.process_user_message(task).await,
            _ => {
                warn!(
                    target: "borg_exec",
                    task_id = %task.task_id,
                    "unsupported task kind in MVP, auto-completing"
                );
                self.db
                    .complete_task(&task.task_id, "Ignored")
                    .await
            }
        }
    }

    async fn process_user_message(&self, task: Task) -> Result<()> {
        let msg: InboxMessage = serde_json::from_value(task.payload.clone())?;

        info!(
            target: "borg_exec",
            task_id = %task.task_id,
            user_key = msg.user_key,
            text = msg.text,
            "processing user message task"
        );

        let tool_runner = ExecToolRunner::new(self.runtime.clone(), search_capabilities(""));
        let tools = AgentTools {
            tool_runner: &tool_runner,
        };
        let mut session = self.session_manager.session_for_task(&msg).await?;
        session
            .add_message(Message::User {
                content: msg.text.clone(),
            })
            .await?;

        let provider = self.configured_provider().await?;

        let output = match session
            .agent
            .clone()
            .run(&mut session, &provider, &tools)
            .await
        {
            SessionResult::Completed(Ok(output)) => output,
            SessionResult::Completed(Err(err)) => {
                return Err(anyhow!("agent session completed with error: {}", err));
            }
            SessionResult::SessionError(err) => {
                return Err(anyhow!("agent session error: {}", err));
            }
            SessionResult::Idle => {
                self.db
                    .log_event(Event::AgentIdle {
                        task_id: task.task_id.clone(),
                    })
                    .await?;
                self.db
                    .complete_task(&task.task_id, "Agent idle")
                    .await?;
                return Ok(());
            }
        };

        debug!(
            target: "borg_exec",
            task_id = %task.task_id,
            tool_calls = output.tool_calls.len(),
            "agent session completed"
        );
        let persisted_messages = session.read_messages(0, usize::MAX).await?;
        trace!(
            target: "borg_exec",
            task_id = %task.task_id,
            messages = ?persisted_messages,
            "persistable session messages"
        );
        for call in &output.tool_calls {
            self.db
                .log_event(Event::AgentToolCall {
                    task_id: task.task_id.clone(),
                    name: call.tool_name.clone(),
                    arguments: call.arguments.clone(),
                    output: serde_json::to_value(&call.output)?,
                })
                .await?;
        }

        let reply = output.reply;
        info!(
            target: "borg_exec",
            task_id = %task.task_id,
            reply = reply.as_str(),
            "agent reply generated"
        );
        self.db
            .log_event(Event::AgentOutput {
                task_id: task.task_id.clone(),
                message: reply.clone(),
            })
            .await?;
        self.db
            .complete_task(
                &task.task_id,
                &reply,
            )
            .await?;

        Ok(())
    }
}
