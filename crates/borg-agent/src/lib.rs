use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_db::BorgDb;
use borg_llm::{
    LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason, ToolDescriptor,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::info;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TURNS: usize = 6;
const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";

pub struct AgentTools<'a> {
    pub tool_runner: &'a dyn ToolRunner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub content: ToolResultData,
}

#[async_trait]
pub trait ToolRunner: Send + Sync {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySummary {
    pub name: String,
    pub signature: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResultData {
    Text(String),
    Capabilities(Vec<CapabilitySummary>),
    Execution {
        result: String,
        duration_ms: u128,
    },
    Error {
        message: String,
    },
}

impl From<&ToolSpec> for ToolDescriptor {
    fn from(value: &ToolSpec) -> Self {
        Self {
            name: value.name.clone(),
            description: value.description.clone(),
            input_schema: value.parameters.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: String,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tools: Vec<ToolSpec>,
}

impl Agent {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            model: DEFAULT_MODEL.to_string(),
            system_prompt: String::new(),
            max_turns: DEFAULT_MAX_TURNS,
            tools: default_tool_specs(),
        }
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = system_prompt.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    pub async fn run<'a, P: Provider>(
        &self,
        session: &mut Session,
        provider: &P,
        tools: &AgentTools<'a>,
    ) -> SessionResult<SessionOutput> {
        if let Err(err) = session.agent_started().await {
            return SessionResult::SessionError(err.to_string());
        }

        let mut pending = session.pop_steering_messages();
        let mut has_tool_calls = match session.has_unprocessed_messages().await {
            Ok(value) => value,
            Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
        };
        let has_unprocessed_user_messages = match session.has_unprocessed_user_messages().await {
            Ok(value) => value,
            Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
        };
        if has_tool_calls && pending.is_empty() && !has_unprocessed_user_messages {
            if let Err(err) = session.mark_processed().await {
                return finish_session(session, SessionResult::SessionError(err.to_string())).await;
            }
            return finish_session(session, SessionResult::Idle).await;
        }
        let mut last_reply = String::new();
        let mut records: Vec<ToolCallRecord> = Vec::new();

        while has_tool_calls || !pending.is_empty() {
            while has_tool_calls || !pending.is_empty() {
                info!(target: "borg_agent", session_id = session.session_id, "turn_start");

                for message in pending.drain(..) {
                    if let Err(err) = session.add_message(message).await {
                        return finish_session(session, SessionResult::SessionError(err.to_string())).await;
                    }
                }

                let messages = match session.read_messages(0, usize::MAX).await {
                    Ok(messages) => messages,
                    Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
                };
                let req = LlmRequest {
                    model: self.model.clone(),
                    messages: to_provider_messages(&messages),
                    tools: to_provider_tool_specs(&self.tools),
                    temperature: None,
                    max_tokens: None,
                    api_key: None,
                };
                let assistant_message = match provider.chat(&req).await {
                    Ok(message) => message,
                    Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
                };
                info!(target: "borg_agent", session_id = session.session_id, "turn_end");

                let tool_calls: Vec<(String, String, Value)> = assistant_message
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ProviderBlock::ToolCall {
                            id,
                            name,
                            arguments_json,
                        } => Some((id.clone(), name.clone(), arguments_json.clone())),
                        _ => None,
                    })
                    .collect();

                if tool_calls.is_empty() {
                    if matches!(assistant_message.stop_reason, StopReason::Aborted | StopReason::Error) {
                        return finish_session(
                            session,
                            SessionResult::SessionError(
                                assistant_message
                                    .error_message
                                    .unwrap_or_else(|| "assistant aborted or errored".to_string()),
                            ),
                        )
                        .await;
                    }

                    last_reply = assistant_message
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ProviderBlock::Text(text) => Some(text.clone()),
                            ProviderBlock::Thinking(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim()
                        .to_string();
                    if let Err(err) = session
                        .add_message(Message::Assistant {
                            content: last_reply.clone(),
                        })
                        .await
                    {
                        return finish_session(session, SessionResult::SessionError(err.to_string())).await;
                    }
                    has_tool_calls = false;
                    pending = session.pop_steering_messages();
                    continue;
                }

                let mut interrupted = false;
                for (tool_call_id, tool_name, arguments) in tool_calls {

                    if let Err(err) = session
                        .add_message(Message::ToolCall {
                            tool_call_id: tool_call_id.clone(),
                            name: tool_name.clone(),
                            arguments: arguments.clone(),
                        })
                        .await
                    {
                        return finish_session(session, SessionResult::SessionError(err.to_string())).await;
                    }

                    info!(target: "borg_agent", session_id = session.session_id, tool_name, "tool_execution_start");
                    let output = match call_tool(tools, &tool_call_id, &tool_name, &arguments).await {
                        Ok(value) => value,
                        Err(err) => ToolResultData::Error {
                            message: err.to_string(),
                        },
                    };
                    info!(target: "borg_agent", session_id = session.session_id, tool_name, "tool_execution_end");

                if let Err(err) = session
                    .add_message(Message::ToolResult {
                        tool_call_id,
                        name: tool_name.clone(),
                        content: output.clone(),
                    })
                    .await
                {
                        return finish_session(session, SessionResult::SessionError(err.to_string())).await;
                    }

                    records.push(ToolCallRecord {
                        tool_name,
                        arguments,
                        output: output.clone(),
                    });

                    let steering = session.pop_steering_messages();
                    if !steering.is_empty() {
                        pending = steering;
                        interrupted = true;
                        break;
                    }
                }

                has_tool_calls = !interrupted;
                if !interrupted {
                    pending = session.pop_steering_messages();
                }
            }

            let follow_ups = session.pop_follow_up_messages();
            if !follow_ups.is_empty() {
                pending = follow_ups;
                has_tool_calls = true;
                continue;
            }

            break;
        }

        if let Err(err) = session.mark_processed().await {
            return finish_session(session, SessionResult::SessionError(err.to_string())).await;
        }

        if last_reply.is_empty() {
            finish_session(session, SessionResult::Idle).await
        } else {
            finish_session(
                session,
                SessionResult::Completed(Ok(SessionOutput {
                    reply: last_reply,
                    tool_calls: records,
                })),
            )
            .await
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    System { content: String },
    User { content: String },
    Assistant { content: String },
    ToolCall {
        tool_call_id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: ToolResultData,
    },
    SessionEvent {
        name: String,
        payload: SessionEventPayload,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEventPayload {
    Started,
    Finished {
        status: SessionEndStatus,
        reply: Option<String>,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEndStatus {
    Completed,
    CompletedError,
    SessionError,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: Value,
    pub output: ToolResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOutput {
    pub reply: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionResult<T> {
    Completed(Result<T, String>),
    SessionError(String),
    Idle,
}

#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    pub agent: Agent,
    db: BorgDb,
    last_processed_len: usize,
    steering_messages: Vec<Message>,
    follow_up_messages: Vec<Message>,
}

impl Session {
    pub async fn new(session_id: impl Into<String>, agent: Agent, db: BorgDb) -> Result<Self> {
        let session_id = session_id.into();
        let mut session = Self {
            session_id: session_id.clone(),
            agent,
            db,
            last_processed_len: 0,
            steering_messages: Vec::new(),
            follow_up_messages: Vec::new(),
        };

        let existing_messages = session.db.count_session_messages(&session_id).await?;
        if existing_messages == 0 && !session.agent.system_prompt.is_empty() {
            session
                .add_message(Message::System {
                    content: session.agent.system_prompt.clone(),
                })
                .await?;
        }
        session.last_processed_len = session.db.count_session_messages(&session_id).await?;
        Ok(session)
    }

    pub async fn add_message(&mut self, message: Message) -> Result<()> {
        let payload = serde_json::to_value(message)?;
        self.db
            .append_session_message(&self.session_id, &payload)
            .await?;
        Ok(())
    }

    pub async fn read_messages(&self, from: usize, limit: usize) -> Result<Vec<Message>> {
        let payloads = self
            .db
            .list_session_messages(&self.session_id, from, limit)
            .await?;
        payloads
            .into_iter()
            .map(serde_json::from_value::<Message>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!(err))
    }

    pub async fn has_unprocessed_messages(&self) -> Result<bool> {
        let count = self.db.count_session_messages(&self.session_id).await?;
        Ok(count > self.last_processed_len)
    }

    pub async fn has_unprocessed_user_messages(&self) -> Result<bool> {
        let messages = self
            .read_messages(self.last_processed_len, usize::MAX)
            .await?;
        Ok(messages
            .into_iter()
            .any(|m| matches!(m, Message::User { .. })))
    }

    pub async fn mark_processed(&mut self) -> Result<()> {
        self.last_processed_len = self.db.count_session_messages(&self.session_id).await?;
        Ok(())
    }

    pub async fn agent_started(&mut self) -> Result<()> {
        self.add_message(Message::SessionEvent {
            name: AGENT_STARTED_EVENT.to_string(),
            payload: SessionEventPayload::Started,
        })
        .await
    }

    pub async fn agent_finished(&mut self, result: &SessionResult<SessionOutput>) -> Result<()> {
        let payload = match result {
            SessionResult::Completed(Ok(output)) => SessionEventPayload::Finished {
                status: SessionEndStatus::Completed,
                reply: Some(output.reply.clone()),
                error: None,
            },
            SessionResult::Completed(Err(err)) => SessionEventPayload::Finished {
                status: SessionEndStatus::CompletedError,
                reply: None,
                error: Some(err.clone()),
            },
            SessionResult::SessionError(err) => SessionEventPayload::Finished {
                status: SessionEndStatus::SessionError,
                reply: None,
                error: Some(err.clone()),
            },
            SessionResult::Idle => SessionEventPayload::Finished {
                status: SessionEndStatus::Idle,
                reply: None,
                error: None,
            },
        };
        self.add_message(Message::SessionEvent {
            name: AGENT_FINISHED_EVENT.to_string(),
            payload,
        })
        .await
    }

    pub fn enqueue_steering_message(&mut self, message: Message) {
        self.steering_messages.push(message);
    }

    pub fn enqueue_follow_up_message(&mut self, message: Message) {
        self.follow_up_messages.push(message);
    }

    pub fn pop_steering_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.steering_messages)
    }

    pub fn pop_follow_up_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.follow_up_messages)
    }
}

async fn finish_session(
    session: &mut Session,
    result: SessionResult<SessionOutput>,
) -> SessionResult<SessionOutput> {
    if let Err(err) = session.agent_finished(&result).await {
        return SessionResult::SessionError(err.to_string());
    }
    result
}

fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "search".to_string(),
            description: "Search capabilities or memory context for the user request".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "execute".to_string(),
            description: "Execute runtime code/action for task fulfillment".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "code": { "type": "string" } },
                "required": ["code"],
                "additionalProperties": false
            }),
        },
    ]
}

fn to_provider_tool_specs(tool_specs: &[ToolSpec]) -> Vec<ToolDescriptor> {
    tool_specs.iter().map(ToolDescriptor::from).collect()
}

fn to_provider_messages(messages: &[Message]) -> Vec<ProviderMessage> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::System { content } => Some(ProviderMessage::System { text: content.clone() }),
            Message::User { content } => Some(ProviderMessage::User {
                content: vec![ProviderBlock::Text(content.clone())],
            }),
            Message::Assistant { content } => Some(ProviderMessage::Assistant {
                content: vec![ProviderBlock::Text(content.clone())],
            }),
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => Some(ProviderMessage::Assistant {
                content: vec![ProviderBlock::ToolCall {
                    id: tool_call_id.clone(),
                    name: name.clone(),
                    arguments_json: arguments.clone(),
                }],
            }),
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => Some(ProviderMessage::ToolResult {
                tool_call_id: tool_call_id.clone(),
                name: name.clone(),
                content: vec![ProviderBlock::Text(tool_result_to_text(content))],
            }),
            Message::SessionEvent { .. } => None,
        })
        .collect()
}

fn tool_result_to_text(content: &ToolResultData) -> String {
    match content {
        ToolResultData::Text(text) => text.clone(),
        ToolResultData::Capabilities(items) => format!("capabilities: {}", items.len()),
        ToolResultData::Execution {
            result,
            duration_ms,
        } => format!("execution result in {}ms: {}", duration_ms, result),
        ToolResultData::Error { message } => format!("tool error: {}", message),
    }
}

async fn call_tool<'a>(
    tools: &AgentTools<'a>,
    tool_call_id: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<ToolResultData> {
    let response = tools
        .tool_runner
        .run(ToolRequest {
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
        })
        .await?;
    Ok(response.content)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::{
        Agent, AgentTools, Message, Session, SessionResult, ToolRequest, ToolResponse, ToolResultData, ToolRunner,
        call_tool,
    };
    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use borg_db::BorgDb;
    use borg_llm::{LlmAssistantMessage, LlmRequest, Provider, ProviderBlock, StopReason};
    use serde_json::{Value, json};
    use uuid::Uuid;

    struct ScriptedRunner {
        calls: Arc<Mutex<Vec<ToolRequest>>>,
        outputs: Arc<Mutex<VecDeque<Result<ToolResponse, String>>>>,
    }

    #[async_trait]
    impl ToolRunner for ScriptedRunner {
        async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
            self.calls.lock().unwrap().push(request);
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow!("missing scripted tool output"))?
                .map_err(|e| anyhow!(e))
        }
    }

    #[derive(Clone)]
    struct ScriptedProvider {
        requests: Arc<Mutex<Vec<LlmRequest>>>,
        responses: Arc<Mutex<VecDeque<Result<LlmAssistantMessage, String>>>>,
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
            self.requests.lock().unwrap().push(req.clone());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow!("missing scripted llm response"))?
                .map_err(|e| anyhow!(e))
        }
    }

    fn assistant_text(text: &str) -> LlmAssistantMessage {
        LlmAssistantMessage {
            content: vec![ProviderBlock::Text(text.to_string())],
            stop_reason: StopReason::EndOfTurn,
            error_message: None,
        }
    }

    fn assistant_tool_calls(calls: Vec<(&str, &str, Value)>) -> LlmAssistantMessage {
        LlmAssistantMessage {
            content: calls
                .into_iter()
                .map(|(id, name, args)| ProviderBlock::ToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    arguments_json: args,
                })
                .collect(),
            stop_reason: StopReason::ToolCall,
            error_message: None,
        }
    }

    async fn make_test_db() -> Result<BorgDb> {
        let path = PathBuf::from(format!("/tmp/borg-agent-test-{}.db", Uuid::now_v7()));
        let borg_db = BorgDb::open_local(path.to_string_lossy().as_ref()).await?;
        borg_db.migrate().await?;
        Ok(borg_db)
    }

    async fn make_session() -> Result<(Agent, Session)> {
        let db = make_test_db().await?;
        let agent = Agent::new("test-agent").with_system_prompt("system prompt");
        let session = Session::new("test-session", agent.clone(), db).await?;
        Ok((agent, session))
    }

    #[tokio::test]
    async fn a1_no_tool_completion() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "hello".to_string(),
            })
            .await
            .unwrap();

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([Ok(assistant_text(
                "hello back",
            ))]))),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::new())),
        };
        let tools = AgentTools {
            tool_runner: &runner,
        };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::Completed(Ok(_))));
    }

    #[tokio::test]
    async fn a2_single_tool_then_answer() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "search and answer".to_string(),
            })
            .await
            .unwrap();

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([
                Ok(assistant_tool_calls(vec![("tc1", "search", json!({"query":"x"}))])),
                Ok(assistant_text("done")),
            ]))),
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = ScriptedRunner {
            calls: calls.clone(),
            outputs: Arc::new(Mutex::new(VecDeque::from([Ok(ToolResponse {
                content: ToolResultData::Text("hits: []".to_string()),
            })]))),
        };
        let tools = AgentTools { tool_runner: &runner };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::Completed(Ok(_))));
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a3_multiple_tools_keep_order() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "run two tools".to_string(),
            })
            .await
            .unwrap();

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([
                Ok(assistant_tool_calls(vec![
                    ("tc1", "search", json!({"query":"one"})),
                    ("tc2", "execute", json!({"code":"1+1"})),
                ])),
                Ok(assistant_text("done")),
            ]))),
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = ScriptedRunner {
            calls: calls.clone(),
            outputs: Arc::new(Mutex::new(VecDeque::from([
                Ok(ToolResponse {
                    content: ToolResultData::Text("a=1".to_string()),
                }),
                Ok(ToolResponse {
                    content: ToolResultData::Text("b=2".to_string()),
                }),
            ]))),
        };
        let tools = AgentTools { tool_runner: &runner };

        let _ = agent.run(&mut session, &provider, &tools).await;
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].tool_name, "search");
        assert_eq!(recorded[1].tool_name, "execute");
    }

    #[tokio::test]
    async fn a5_follow_up_continues_run() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "first".to_string(),
            })
            .await
            .unwrap();
        session.enqueue_follow_up_message(Message::User {
            content: "follow-up".to_string(),
        });

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([
                Ok(assistant_text("turn-1")),
                Ok(assistant_text("turn-2")),
            ]))),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::new())),
        };
        let tools = AgentTools { tool_runner: &runner };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::Completed(Ok(_))));
        let msgs = session.read_messages(0, 256).await.unwrap();
        assert!(msgs
            .iter()
            .any(|m| matches!(m, Message::User { content } if content == "follow-up")));
    }

    #[tokio::test]
    async fn a6_tool_error_is_returned_as_tool_result() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "tool error path".to_string(),
            })
            .await
            .unwrap();

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([
                Ok(assistant_tool_calls(vec![("tc1", "execute", json!({"code":"bad"}))])),
                Ok(assistant_text("handled")),
            ]))),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::from([Err(
                "execution failed".to_string(),
            )]))),
        };
        let tools = AgentTools { tool_runner: &runner };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::Completed(Ok(_))));
        let msgs = session.read_messages(0, 256).await.unwrap();
        assert!(msgs.iter().any(|m| {
            matches!(m, Message::ToolResult { content: ToolResultData::Error { .. }, .. })
        }));
    }

    #[tokio::test]
    async fn a7_provider_failure_surfaces_session_error() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "provider fail".to_string(),
            })
            .await
            .unwrap();

        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([Err(
                "provider down".to_string(),
            )]))),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::new())),
        };
        let tools = AgentTools { tool_runner: &runner };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::SessionError(_)));
    }

    #[tokio::test]
    async fn a8_idle_run_when_no_new_messages() {
        let (agent, mut session) = make_session().await.unwrap();
        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::new())),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::new())),
        };
        let tools = AgentTools { tool_runner: &runner };

        let result = agent.run(&mut session, &provider, &tools).await;
        assert!(matches!(result, SessionResult::Idle));
    }

    #[tokio::test]
    async fn a9_lifecycle_events_persisted_once() {
        let (agent, mut session) = make_session().await.unwrap();
        session
            .add_message(Message::User {
                content: "hello".to_string(),
            })
            .await
            .unwrap();
        let provider = ScriptedProvider {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from([Ok(assistant_text("ok"))]))),
        };
        let runner = ScriptedRunner {
            calls: Arc::new(Mutex::new(Vec::new())),
            outputs: Arc::new(Mutex::new(VecDeque::new())),
        };
        let tools = AgentTools { tool_runner: &runner };

        let _ = agent.run(&mut session, &provider, &tools).await;
        let messages = session.read_messages(0, 256).await.unwrap();
        let started = messages
            .iter()
            .filter(|m| {
                matches!(m, Message::SessionEvent { name, .. } if name == "agent_started")
            })
            .count();
        let finished = messages
            .iter()
            .filter(|m| {
                matches!(m, Message::SessionEvent { name, .. } if name == "agent_finished")
            })
            .count();
        assert_eq!(started, 1);
        assert_eq!(finished, 1);
    }

    #[tokio::test]
    async fn injected_tool_runner_helper_still_works() {
        struct InlineRunner;
        #[async_trait]
        impl ToolRunner for InlineRunner {
            async fn run(&self, _request: ToolRequest) -> Result<ToolResponse> {
                Ok(ToolResponse {
                    content: ToolResultData::Text("ok".to_string()),
                })
            }
        }

        let tools = AgentTools {
            tool_runner: &InlineRunner,
        };
        let out = call_tool(
            &tools,
            "tc1",
            "search",
            &json!({"query": "x"}),
        )
        .await
            .unwrap();
        assert!(matches!(out, ToolResultData::Text(text) if text == "ok"));
    }
}
