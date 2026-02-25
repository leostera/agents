use anyhow::{Context, Result};
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
use tracing::{debug, trace};
use tracing::{error, info, warn};
use uuid::Uuid;

const OPENAI_PROVIDER: &str = "openai";

#[derive(Clone)]
pub struct ExecEngine {
    db: BorgDb,
    runtime: RuntimeEngine,
    worker_id: String,
}

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
                    .ok_or_else(|| anyhow::anyhow!("execute tool requires code"))?;
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
                    .ok_or_else(|| anyhow::anyhow!("search tool requires query"))?;
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

impl ExecEngine {
    pub fn new(db: BorgDb, runtime: RuntimeEngine, worker_id: String) -> Self {
        Self {
            db,
            runtime,
            worker_id,
        }
    }

    pub async fn enqueue_user_message(
        &self,
        mut msg: InboxMessage,
        requested_session_id: Option<String>,
    ) -> Result<(String, String)> {
        let session_id = requested_session_id.unwrap_or_else(|| format!("borg:session:{}", Uuid::now_v7()));
        msg.session_id = Some(session_id.clone());
        let payload = serde_json::to_value(msg).context("failed to serialize inbox message")?;
        let task_id = self
            .db
            .enqueue_task(NewTask {
                kind: TaskKind::UserMessage,
                payload,
                parent_task_id: None,
            })
            .await?;

        Ok((task_id, session_id))
    }

    pub async fn run_once(&self) -> Result<bool> {
        let Some(task) = self.db.claim_next_runnable_task(&self.worker_id).await? else {
            return Ok(false);
        };

        info!(target: "borg_exec", task_id = task.task_id, kind = task.kind.as_str(), "claimed task for execution");
        self.db
            .log_event(
                &task.task_id,
                "task_claimed",
                json!({ "worker_id": self.worker_id }),
            )
            .await?;

        match self.process_task(task.clone()).await {
            Ok(()) => {
                info!(target: "borg_exec", task_id = task.task_id, "task execution completed successfully");
            }
            Err(err) => {
                error!(target: "borg_exec", task_id = task.task_id, error = %err, "task execution failed");
                self.db.fail_task(&task.task_id, err.to_string()).await?;
            }
        }

        Ok(true)
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
                warn!(target: "borg_exec", task_id = task.task_id, "unsupported task kind in MVP, auto-completing");
                self.db
                    .complete_task(&task.task_id, json!({"status": "ignored"}))
                    .await
            }
        }
    }

    async fn process_user_message(&self, task: Task) -> Result<()> {
        let msg: InboxMessage = serde_json::from_value(task.payload.clone())
            .context("invalid payload for user_message task")?;

        info!(target: "borg_exec", task_id = task.task_id, user_key = msg.user_key, text = msg.text, "processing user message task");
        let tool_runner = ExecToolRunner {
            runtime: self.runtime.clone(),
            capabilities: self.search_capabilities(""),
        };
        let tools = AgentTools {
            tool_runner: &tool_runner,
        };
        let agent = Agent::new("borg-default").with_system_prompt(
            "You are Borg's agent runtime. Use tools as needed, then respond clearly.",
        );
        let agent_runner = agent.clone();
        let session_id = msg
            .session_id
            .clone()
            .unwrap_or_else(|| format!("borg:session:{}", Uuid::now_v7()));
        let mut session = Session::new(session_id.clone(), agent, self.db.clone()).await?;
        session
            .add_message(Message::User {
                content: msg.text.clone(),
            })
            .await?;

        let api_key = self
            .db
            .get_provider_api_key(OPENAI_PROVIDER)
            .await?
            .ok_or_else(|| anyhow::anyhow!("OpenAI provider is not configured"))?;
        let provider = OpenAiProvider::new(api_key);

        let output = match agent_runner.run(&mut session, &provider, &tools).await {
            SessionResult::Completed(Ok(output)) => output,
            SessionResult::Completed(Err(err)) => {
                return Err(anyhow::anyhow!(
                    "agent session completed with error: {}",
                    err
                ));
            }
            SessionResult::SessionError(err) => {
                return Err(anyhow::anyhow!("agent session error: {}", err));
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
        debug!(target: "borg_exec", task_id = task.task_id, tool_calls = output.tool_calls.len(), "agent session completed");
        let persisted_messages = session.read_messages(0, usize::MAX).await?;
        trace!(target: "borg_exec", task_id = task.task_id, messages = ?persisted_messages, "persistable session messages");
        self.db
            .log_event(
                &task.task_id,
                "agent_tool_calls",
                json!({ "calls": output.tool_calls }),
            )
            .await?;

        let output = output.reply;
        info!(target: "borg_exec", task_id = task.task_id, "agent reply generated");
        self.db
            .log_event(&task.task_id, "output", json!({ "message": output }))
            .await?;
        self.db
            .complete_task(&task.task_id, json!({ "message": output }))
            .await?;

        Ok(())
    }
}
