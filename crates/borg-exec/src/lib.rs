use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_agent::{
    Agent, AgentTools, CapabilitySummary, Message, Session, SessionResult, ToolRequest,
    ToolResponse, ToolResultData, ToolRunner,
};
use borg_core::{Capability, Task, TaskKind};
use borg_db::{BorgDb, NewTask};
use borg_llm::providers::openai::OpenAiProvider;
use borg_rt::RuntimeEngine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

const OPENAI_PROVIDER: &str = "openai";

#[derive(Clone)]
pub struct TaskQueue {
    sender: UnboundedSender<String>,
    receiver: Arc<Mutex<UnboundedReceiver<String>>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel::<String>();
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub async fn queue(&self, task_id: String) -> Result<()> {
        self.sender
            .send(task_id)
            .map_err(|_| anyhow!("task queue is closed"))
    }

    pub async fn next(&self) -> Result<String> {
        let mut receiver = self.receiver.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| anyhow!("task queue receiver is closed"))
    }
}

#[derive(Clone)]
pub struct SessionManager {
    db: BorgDb,
}

impl SessionManager {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    pub async fn session_for_task(&self, msg: &InboxMessage) -> Result<Session> {
        let session_id = msg
            .session_id
            .clone()
            .unwrap_or_else(|| format!("borg:session:{}", Uuid::now_v7()));
        let agent = Agent::new("borg-default").with_system_prompt(
            "You are Borg's agent runtime. Use tools as needed, then respond clearly.",
        );
        Session::new(session_id, agent, self.db.clone()).await
    }
}

#[derive(Clone)]
pub struct BorgExecutor {
    db: BorgDb,
    runtime: RuntimeEngine,
    worker_id: String,
    task_queue: TaskQueue,
    session_manager: SessionManager,
}

pub type ExecEngine = BorgExecutor;

struct ExecToolRunner {
    runtime: RuntimeEngine,
    capabilities: Vec<Capability>,
}

#[async_trait]
impl ToolRunner for ExecToolRunner {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
        match request.tool_name.as_str() {
            "execute" => {
                let code = request
                    .arguments
                    .get("code")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("execute tool requires code"))?;
                let result = self.runtime.execute(code)?;
                Ok(ToolResponse {
                    content: ToolResultData::Execution {
                        result: result.result_json.to_string(),
                        duration_ms: result.duration_ms,
                    },
                })
            }
            "search" => {
                let query = request
                    .arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("search tool requires query"))?;
                let q = query.to_lowercase();
                let matches: Vec<Capability> = self
                    .capabilities
                    .iter()
                    .filter(|cap| {
                        cap.name.contains(&q) || cap.description.to_lowercase().contains(&q)
                    })
                    .cloned()
                    .collect();
                let result = if matches.is_empty() {
                    self.capabilities.clone()
                } else {
                    matches
                };
                Ok(ToolResponse {
                    content: ToolResultData::Capabilities(
                        result
                            .into_iter()
                            .map(|cap| CapabilitySummary {
                                name: cap.name,
                                signature: cap.signature,
                                description: cap.description,
                            })
                            .collect(),
                    ),
                })
            }
            _ => Ok(ToolResponse {
                content: ToolResultData::Error {
                    message: format!("unknown tool {}", request.tool_name),
                },
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboxMessage {
    pub user_key: String,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

impl BorgExecutor {
    pub fn new(db: BorgDb, runtime: RuntimeEngine, worker_id: String) -> Self {
        let task_queue = TaskQueue::new();
        let session_manager = SessionManager::new(db.clone());
        Self {
            db,
            runtime,
            worker_id,
            task_queue,
            session_manager,
        }
    }

    pub async fn enqueue_user_message(
        &self,
        mut msg: InboxMessage,
        requested_session_id: Option<String>,
    ) -> Result<(String, String)> {
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

    pub async fn run(self) -> Result<()> {
        self.recover_tasks_on_startup().await?;
        info!(
            target: "borg_exec",
            worker_id = self.worker_id,
            "executor loop started"
        );

        loop {
            let task_id = self.task_queue.next().await?;
            if let Err(err) = self.process_task_id(&task_id).await {
                error!(
                    target: "borg_exec",
                    task_id,
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

    async fn process_task_id(&self, task_id: &str) -> Result<()> {
        let Some(task) = self.db.claim_task_by_id(&self.worker_id, task_id).await? else {
            debug!(
                target: "borg_exec",
                task_id,
                "task was not claimable when popped from queue"
            );
            return Ok(());
        };

        info!(
            target: "borg_exec",
            task_id = task.task_id,
            kind = task.kind.as_str(),
            "claimed task for execution"
        );
        self.db
            .log_event(
                &task.task_id,
                "task_claimed",
                json!({ "worker_id": self.worker_id }),
            )
            .await?;

        match self.process_task(task.clone()).await {
            Ok(()) => {
                info!(
                    target: "borg_exec",
                    task_id = task.task_id,
                    "task execution completed successfully"
                );
            }
            Err(err) => {
                error!(
                    target: "borg_exec",
                    task_id = task.task_id,
                    error = %err,
                    "task execution failed"
                );
                self.db.fail_task(&task.task_id, err.to_string()).await?;
            }
        }

        Ok(())
    }

    fn search_capabilities(&self, query: &str) -> Vec<Capability> {
        let q = query.to_lowercase();
        let catalog = vec![
            Capability {
                name: "torrents.search".to_string(),
                signature: "(query: string) => Promise<TorrentResult[]>".to_string(),
                description: "Searches torrent providers by title keywords".to_string(),
            },
            Capability {
                name: "torrents.download".to_string(),
                signature: "(magnet: string, dest: string) => Promise<DownloadReceipt>".to_string(),
                description: "Downloads a magnet link into a destination path".to_string(),
            },
            Capability {
                name: "memory.upsert".to_string(),
                signature: "(entity: Entity) => Promise<string>".to_string(),
                description: "Upserts an entity into long-term memory".to_string(),
            },
            Capability {
                name: "memory.link".to_string(),
                signature: "(from: string, rel: string, to: string) => Promise<string>".to_string(),
                description: "Creates a relation between entities".to_string(),
            },
        ];

        let filtered: Vec<Capability> = catalog
            .clone()
            .into_iter()
            .filter(|c| c.name.contains(&q) || c.description.to_lowercase().contains(&q))
            .collect();

        if filtered.is_empty() {
            catalog
        } else {
            filtered
        }
    }

    async fn process_task(&self, task: Task) -> Result<()> {
        match task.kind {
            TaskKind::UserMessage => self.process_user_message(task).await,
            _ => {
                warn!(
                    target: "borg_exec",
                    task_id = task.task_id,
                    "unsupported task kind in MVP, auto-completing"
                );
                self.db
                    .complete_task(&task.task_id, json!({"status": "ignored"}))
                    .await
            }
        }
    }

    async fn process_user_message(&self, task: Task) -> Result<()> {
        let msg: InboxMessage = serde_json::from_value(task.payload.clone())?;

        info!(
            target: "borg_exec",
            task_id = task.task_id,
            user_key = msg.user_key,
            text = msg.text,
            "processing user message task"
        );

        let tool_runner = ExecToolRunner {
            runtime: self.runtime.clone(),
            capabilities: self.search_capabilities(""),
        };
        let tools = AgentTools {
            tool_runner: &tool_runner,
        };
        let mut session = self.session_manager.session_for_task(&msg).await?;
        session
            .add_message(Message::User {
                content: msg.text.clone(),
            })
            .await?;

        let api_key = self
            .db
            .get_provider_api_key(OPENAI_PROVIDER)
            .await?
            .ok_or_else(|| anyhow!("OpenAI provider is not configured"))?;
        let provider = OpenAiProvider::new(api_key);

        let output = match session.agent.clone().run(&mut session, &provider, &tools).await {
            SessionResult::Completed(Ok(output)) => output,
            SessionResult::Completed(Err(err)) => {
                return Err(anyhow!("agent session completed with error: {}", err));
            }
            SessionResult::SessionError(err) => {
                return Err(anyhow!("agent session error: {}", err));
            }
            SessionResult::Idle => {
                self.db
                    .log_event(&task.task_id, "agent_idle", json!({}))
                    .await?;
                self.db
                    .complete_task(&task.task_id, json!({ "message": "Agent idle" }))
                    .await?;
                return Ok(());
            }
        };

        debug!(
            target: "borg_exec",
            task_id = task.task_id,
            tool_calls = output.tool_calls.len(),
            "agent session completed"
        );
        let persisted_messages = session.read_messages(0, usize::MAX).await?;
        trace!(
            target: "borg_exec",
            task_id = task.task_id,
            messages = ?persisted_messages,
            "persistable session messages"
        );
        self.db
            .log_event(
                &task.task_id,
                "agent_tool_calls",
                json!({ "calls": output.tool_calls }),
            )
            .await?;

        let reply = output.reply;
        info!(target: "borg_exec", task_id = task.task_id, "agent reply generated");
        self.db
            .log_event(&task.task_id, "output", json!({ "message": reply }))
            .await?;
        self.db
            .complete_task(&task.task_id, json!({ "message": reply }))
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_core::TaskStatus;
    use std::path::PathBuf;
    use std::time::Duration;
    use uuid::Uuid;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!("borg-exec-test-{}.db", Uuid::now_v7()))
    }

    async fn open_test_db() -> BorgDb {
        let path = temp_db_path();
        let db = BorgDb::open_local(path.to_str().unwrap()).await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    async fn wait_for_task_status(
        db: &BorgDb,
        task_id: &str,
        status: TaskStatus,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();
        loop {
            let task = db.get_task(task_id).await.unwrap();
            if let Some(task) = task {
                if task.status == status {
                    return true;
                }
            }
            if start.elapsed() >= timeout {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn enqueue_user_message_persists_task_and_session() {
        let db = open_test_db().await;
        let exec = BorgExecutor::new(db.clone(), RuntimeEngine::default(), "worker-test".to_string());

        let msg = InboxMessage {
            user_key: "u1".to_string(),
            text: "hello".to_string(),
            session_id: None,
            metadata: json!({}),
        };
        let (task_id, session_id) = exec.enqueue_user_message(msg, None).await.unwrap();

        let task = db.get_task(&task_id).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(task.kind, TaskKind::UserMessage);
        assert!(session_id.starts_with("borg:session:"));
    }

    #[tokio::test]
    async fn run_processes_enqueued_task_and_marks_failed_without_provider() {
        let db = open_test_db().await;
        let exec = BorgExecutor::new(
            db.clone(),
            RuntimeEngine::default(),
            "worker-no-provider".to_string(),
        );

        let runner = exec.clone();
        let handle = tokio::spawn(async move { runner.run().await.unwrap() });

        let msg = InboxMessage {
            user_key: "u2".to_string(),
            text: "hello from test".to_string(),
            session_id: None,
            metadata: json!({}),
        };
        let (task_id, _) = exec.enqueue_user_message(msg, None).await.unwrap();

        let done = wait_for_task_status(
            &db,
            &task_id,
            TaskStatus::Failed,
            Duration::from_secs(5),
        )
        .await;
        assert!(done);

        let events = db.get_task_events(&task_id).await.unwrap();
        assert!(events.iter().any(|e| e.event_type == "task_created"));
        assert!(events.iter().any(|e| e.event_type == "task_claimed"));
        assert!(events.iter().any(|e| e.event_type == "task_failed"));

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn run_recovers_running_tasks_on_startup_and_executes_them() {
        let db = open_test_db().await;

        let payload = json!(InboxMessage {
            user_key: "u3".to_string(),
            text: "recover me".to_string(),
            session_id: Some(format!("borg:session:{}", Uuid::now_v7())),
            metadata: json!({}),
        });
        let task_id = db
            .enqueue_task(NewTask {
                kind: TaskKind::UserMessage,
                payload,
                parent_task_id: None,
            })
            .await
            .unwrap();

        let claimed = db.claim_next_runnable_task("old-worker").await.unwrap().unwrap();
        assert_eq!(claimed.task_id, task_id);
        assert_eq!(claimed.status, TaskStatus::Running);

        let exec = BorgExecutor::new(
            db.clone(),
            RuntimeEngine::default(),
            "recovery-worker".to_string(),
        );
        let handle = tokio::spawn(async move { exec.run().await.unwrap() });

        let done = wait_for_task_status(
            &db,
            &task_id,
            TaskStatus::Failed,
            Duration::from_secs(5),
        )
        .await;
        assert!(done);

        let task = db.get_task(&task_id).await.unwrap().unwrap();
        assert_eq!(task.claimed_by.unwrap(), "recovery-worker");
        assert!(task.last_error.unwrap().contains("OpenAI provider is not configured"));

        handle.abort();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn run_reuses_explicit_session_id_across_multiple_tasks() {
        let db = open_test_db().await;
        let exec = BorgExecutor::new(
            db.clone(),
            RuntimeEngine::default(),
            "worker-session".to_string(),
        );

        let runner = exec.clone();
        let handle = tokio::spawn(async move { runner.run().await.unwrap() });

        let session_id = format!("borg:session:{}", Uuid::now_v7());
        let first = InboxMessage {
            user_key: "u4".to_string(),
            text: "first".to_string(),
            session_id: None,
            metadata: json!({}),
        };
        let second = InboxMessage {
            user_key: "u4".to_string(),
            text: "second".to_string(),
            session_id: None,
            metadata: json!({}),
        };

        let (task_a, returned_session_a) = exec
            .enqueue_user_message(first, Some(session_id.clone()))
            .await
            .unwrap();
        let (task_b, returned_session_b) = exec
            .enqueue_user_message(second, Some(session_id.clone()))
            .await
            .unwrap();

        assert_eq!(returned_session_a, session_id);
        assert_eq!(returned_session_b, session_id);

        let done_a = wait_for_task_status(
            &db,
            &task_a,
            TaskStatus::Failed,
            Duration::from_secs(5),
        )
        .await;
        let done_b = wait_for_task_status(
            &db,
            &task_b,
            TaskStatus::Failed,
            Duration::from_secs(5),
        )
        .await;
        assert!(done_a);
        assert!(done_b);

        let message_count = db.count_session_messages(&session_id).await.unwrap();
        assert!(message_count >= 2);

        handle.abort();
        let _ = handle.await;
    }
}
