use anyhow::{Result, anyhow};
use borg_agent::{AgentTools, ContextWindow, Message, SessionResult, ToolSpec};
use borg_core::{
    Event, SessionContextSnapshot, SessionToolSchema, Task, TaskEvent, TaskKind, TaskStatus, Uri,
    uri,
};
use borg_db::{BorgDb, NewTask};
use borg_llm::providers::configured::{ConfiguredProvider, ProviderSettings};
use borg_rt::CodeModeRuntime;
use serde_json::{Value, json};
use tracing::{debug, error, info, trace, warn};

use crate::session_manager::SessionManager;
use crate::task_queue::TaskQueue;
use crate::tool_runner::build_exec_toolchain;
use crate::types::{SessionTurnOutput, UserMessage};
use crate::port_context::{PortContext, TelegramSessionContext};

const OPENAI_PROVIDER: &str = "openai";
const QUEUED_RETRY_DELAY_MILLIS: u64 = 100;
const DEFAULT_AGENT_MODEL: &str = "gpt-4o-mini";
const STARTUP_REQUEUE_MAX_RETRIES: u8 = 5;
const CONTEXT_USAGE_CHAR_TO_TOKEN_RATIO: usize = 4;

#[derive(Clone)]
pub struct BorgExecutor {
    db: BorgDb,
    runtime: CodeModeRuntime,
    worker_id: Uri,
    task_queue: TaskQueue,
    session_manager: SessionManager,
    openai_base_url: Option<String>,
    agent_model: String,
}

pub type ExecEngine = BorgExecutor;

impl BorgExecutor {
    pub fn new(db: BorgDb, runtime: CodeModeRuntime, worker_id: Uri) -> Self {
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
        mut msg: UserMessage,
        requested_session_id: Option<Uri>,
    ) -> Result<(Uri, Uri)> {
        let session_id = requested_session_id.unwrap_or_else(|| uri!("borg", "session"));
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

    pub async fn process_port_message(
        &self,
        port: &str,
        mut msg: UserMessage,
    ) -> Result<SessionTurnOutput> {
        let (session_id, bound_agent_id) = self
            .db
            .resolve_port_session(
                port,
                &msg.user_key,
                msg.session_id.as_ref(),
                msg.agent_id.as_ref(),
            )
            .await?;
        msg.session_id = Some(session_id.clone());
        if msg.agent_id.is_none() {
            msg.agent_id = bound_agent_id;
        }

        info!(
            target: "borg_exec",
            port,
            session_id = %session_id,
            user_key = %msg.user_key,
            "processing inbound port message on long-lived session"
        );

        self.merge_port_message_metadata(port, &session_id, &msg.metadata)
            .await?;
        let output = self.run_session_turn(&msg, None).await?;
        Ok(SessionTurnOutput {
            session_id,
            reply: output.as_ref().map(|value| value.reply.clone()),
            tool_calls: output
                .as_ref()
                .map(|value| {
                    value
                        .tool_calls
                        .iter()
                        .map(|call| format!("{} {}", call.tool_name, call.arguments))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub async fn get_task_events(&self, task_id: &Uri) -> Result<Vec<TaskEvent>> {
        self.db.get_task_events(task_id).await
    }

    pub async fn estimate_session_context_usage_percent(
        &self,
        session_id: &Uri,
        max_context_tokens: usize,
    ) -> Result<usize> {
        let messages = self.db.list_session_messages(session_id, 0, 10_000).await?;
        let total_chars: usize = messages
            .iter()
            .map(|message| message.to_string().chars().count())
            .sum();
        let estimated_tokens = total_chars / CONTEXT_USAGE_CHAR_TO_TOKEN_RATIO;
        let max_tokens = max_context_tokens.max(1);
        let percent = ((estimated_tokens.saturating_mul(100)) / max_tokens).min(100);
        Ok(percent)
    }

    pub async fn list_session_messages(
        &self,
        session_id: &Uri,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Value>> {
        self.db.list_session_messages(session_id, from, limit).await
    }

    pub async fn compact_session(&self, session_id: &Uri) -> Result<usize> {
        let context = self.context_window_for_session(session_id).await?;
        self.db.clear_session_history(session_id).await?;
        for message in &context.messages {
            let payload = serde_json::to_value(message)?;
            self.db.append_session_message(session_id, &payload).await?;
        }
        Ok(context.messages.len())
    }

    pub async fn context_window_for_session(&self, session_id: &Uri) -> Result<ContextWindow> {
        let synthetic_msg = UserMessage {
            user_key: Uri::from_parts("borg", "user", Some("system"))?,
            text: String::new(),
            session_id: Some(session_id.clone()),
            agent_id: None,
            metadata: json!({}),
        };
        let session = self.session_manager.session_for_task(&synthetic_msg).await?;
        let context = session.build_context().await?;
        Ok(context)
    }

    pub async fn get_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
    ) -> Result<Option<Value>> {
        self.db.get_port_session_context(port, session_id).await
    }

    pub async fn upsert_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
        ctx: &Value,
    ) -> Result<()> {
        self.db.upsert_port_session_context(port, session_id, ctx).await
    }

    pub async fn list_port_session_ids(&self, port: &str) -> Result<Vec<Uri>> {
        self.db.list_port_session_ids(port).await
    }

    pub async fn merge_port_message_metadata(
        &self,
        port: &str,
        session_id: &Uri,
        metadata: &Value,
    ) -> Result<()> {
        self.merge_port_context(port, session_id, metadata).await
    }

    pub async fn clear_session_history(&self, session_id: &Uri) -> Result<u64> {
        self.db.clear_session_history(session_id).await
    }

    pub async fn clear_port_session_context(&self, port: &str, session_id: &Uri) -> Result<u64> {
        self.db.clear_port_session_context(port, session_id).await
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
            let exec = self.clone();
            let task_id_for_worker = task_id.clone();
            let join = tokio::spawn(async move { exec.process_task_id(&task_id_for_worker).await });
            match join.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    error!(
                        target: "borg_exec",
                        task_id = %task_id,
                        error = %err,
                        "executor task processing failed"
                    );
                }
                Err(err) => {
                    error!(
                        target: "borg_exec",
                        task_id = %task_id,
                        error = %err,
                        "executor task panicked; task will be marked failed and loop will continue"
                    );
                    let _ = self
                        .db
                        .fail_task(&task_id, &format!("executor panic: {}", err))
                        .await;
                }
            }
        }
    }

    async fn recover_tasks_on_startup(&self) -> Result<()> {
        let mut requeued = None;
        let mut last_error = None;
        for attempt in 0..STARTUP_REQUEUE_MAX_RETRIES {
            match self.db.requeue_running_tasks().await {
                Ok(count) => {
                    requeued = Some(count);
                    break;
                }
                Err(err) => {
                    last_error = Some(err);
                    let backoff = QUEUED_RETRY_DELAY_MILLIS * u64::from(attempt + 1);
                    warn!(
                        target: "borg_exec",
                        attempt = attempt + 1,
                        retries = STARTUP_REQUEUE_MAX_RETRIES,
                        delay_ms = backoff,
                        "failed to requeue running tasks at startup, retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                }
            }
        }

        let requeued = match requeued {
            Some(count) => count,
            None => return Err(last_error.expect("startup requeue error is always set")),
        };

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
        let claimed_task = match self.db.claim_task_by_id(&self.worker_id, task_id).await {
            Ok(task) => task,
            Err(err) => {
                warn!(
                    target: "borg_exec",
                    task_id = %task_id,
                    error = %err,
                    delay_ms = QUEUED_RETRY_DELAY_MILLIS,
                    "failed to claim task by id, requeueing"
                );
                tokio::time::sleep(std::time::Duration::from_millis(QUEUED_RETRY_DELAY_MILLIS))
                    .await;
                self.task_queue.queue(task_id.clone()).await?;
                return Ok(());
            }
        };

        let Some(task) = claimed_task else {
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
                self.db.fail_task(&task.task_id, &err.to_string()).await?;
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
                self.db.complete_task(&task.task_id, "Ignored").await
            }
        }
    }

    async fn process_user_message(&self, task: Task) -> Result<()> {
        let msg: UserMessage = serde_json::from_value(task.payload.clone())?;
        let task_id = task.task_id.clone();

        info!(
            target: "borg_exec",
            task_id = %task_id,
            user_key = %msg.user_key,
            text = msg.text,
            "processing user message task"
        );

        let output = self.run_session_turn(&msg, Some(&task_id)).await?;
        match output {
            Some(output) => {
                self.db.complete_task(&task_id, &output.reply).await?;
            }
            None => {
                self.db.complete_task(&task_id, "Agent idle").await?;
            }
        }
        Ok(())
    }

    async fn run_session_turn(
        &self,
        msg: &UserMessage,
        task_id: Option<&Uri>,
    ) -> Result<Option<borg_agent::SessionOutput>> {
        let toolchain = build_exec_toolchain(self.runtime.clone())?;
        let tools = AgentTools {
            tool_runner: &toolchain,
        };
        let mut session = self.session_manager.session_for_task(msg).await?;
        let session_id = session.session_id.clone();
        let before_messages = session.read_messages(0, usize::MAX).await?;
        if let Some(task_id) = task_id {
            let agent_id = session.agent.agent_id.clone();
            if before_messages.len() <= 1 {
                self.db
                    .log_event(Event::SessionStarted {
                        task_id: task_id.clone(),
                        session_id: session_id.clone(),
                        agent_id,
                    })
                    .await?;
                self.log_session_messages(task_id, &session_id, 0, &before_messages)
                    .await?;
            }
        }

        session
            .add_message(Message::User {
                content: msg.text.clone(),
            })
            .await?;
        let context = session.build_context().await?;
        if let Some(task_id) = task_id {
            self.log_context_built(task_id, &session_id, &session.agent.model, &context)
                .await?;
        }

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
                if let Some(task_id) = task_id {
                    let after_messages = session.read_messages(0, usize::MAX).await?;
                    self.log_session_messages(
                        task_id,
                        &session_id,
                        before_messages.len(),
                        &after_messages,
                    )
                    .await?;
                    self.db
                        .log_event(Event::AgentIdle {
                            task_id: task_id.clone(),
                        })
                        .await?;
                    self.db
                        .log_event(Event::LlmResponseReceived {
                            task_id: task_id.clone(),
                            session_id: session_id.clone(),
                            stop_reason: "idle".to_string(),
                            content_blocks: 0,
                            tool_call_count: 0,
                        })
                        .await?;
                }
                return Ok(None);
            }
        };

        if let Some(task_id) = task_id {
            debug!(
                target: "borg_exec",
                task_id = %task_id,
                tool_calls = output.tool_calls.len(),
                "agent session completed"
            );
        }
        let persisted_messages = session.read_messages(0, usize::MAX).await?;
        if let Some(task_id) = task_id {
            self.log_session_messages(
                task_id,
                &session_id,
                before_messages.len(),
                &persisted_messages,
            )
            .await?;
            trace!(
                target: "borg_exec",
                task_id = %task_id,
                messages = ?persisted_messages,
                "persistable session messages"
            );
            for call in &output.tool_calls {
                self.db
                    .log_event(Event::AgentToolCall {
                        task_id: task_id.clone(),
                        name: call.tool_name.clone(),
                        arguments: call.arguments.clone(),
                        output: serde_json::to_value(&call.output)?,
                    })
                    .await?;
            }

            let reply = output.reply.clone();
            info!(
                target: "borg_exec",
                task_id = %task_id,
                reply = reply.as_str(),
                "agent reply generated"
            );
            self.db
                .log_event(Event::AgentOutput {
                    task_id: task_id.clone(),
                    message: reply,
                })
                .await?;
            self.db
                .log_event(Event::LlmResponseReceived {
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    stop_reason: "completed".to_string(),
                    content_blocks: 1,
                    tool_call_count: output.tool_calls.len(),
                })
                .await?;
        } else {
            info!(
                target: "borg_exec",
                session_id = %session_id,
                "agent session turn completed on long-lived session"
            );
        }

        Ok(Some(output))
    }

    async fn merge_port_context(&self, port: &str, session_id: &Uri, metadata: &Value) -> Result<()> {
        if port != "telegram" {
            return Ok(());
        }
        let maybe_existing = self.db.get_port_session_context("telegram", session_id).await?;
        let mut ctx = match maybe_existing {
            Some(value) => TelegramSessionContext::from_json(value)?,
            None => TelegramSessionContext::default(),
        };
        ctx.merge_message_metadata(metadata)?;
        self.db
            .upsert_port_session_context("telegram", session_id, &ctx.to_json()?)
            .await?;
        Ok(())
    }

    async fn log_session_messages(
        &self,
        task_id: &Uri,
        session_id: &Uri,
        from_index: usize,
        messages: &[Message],
    ) -> Result<()> {
        for (index, message) in messages.iter().enumerate().skip(from_index) {
            self.db
                .log_event(Event::SessionMessage {
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    index,
                    message: serde_json::to_value(message)?,
                })
                .await?;
        }
        Ok(())
    }

    async fn log_context_built(
        &self,
        task_id: &Uri,
        session_id: &Uri,
        model: &str,
        context: &ContextWindow,
    ) -> Result<()> {
        let tools = context
            .tools
            .iter()
            .map(to_session_tool_schema)
            .collect::<Vec<_>>();
        let messages = context
            .messages
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        self.db
            .log_event(Event::ContextBuilt {
                task_id: task_id.clone(),
                session_id: session_id.clone(),
                context: SessionContextSnapshot {
                    model: model.to_string(),
                    messages,
                    tools,
                },
            })
            .await?;
        self.db
            .log_event(Event::LlmRequestSent {
                task_id: task_id.clone(),
                session_id: session_id.clone(),
                model: model.to_string(),
                message_count: context.messages.len(),
                tool_count: context.tools.len(),
            })
            .await
    }
}

fn to_session_tool_schema(tool: &ToolSpec) -> SessionToolSchema {
    SessionToolSchema {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.parameters.clone(),
    }
}
