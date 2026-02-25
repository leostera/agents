use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_db::BorgDb;
use borg_llm::{
    LlmRequest, Provider, ProviderBlock, ProviderMessage, StopReason, ToolDescriptor, UserBlock,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::info;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TURNS: usize = 6;
const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";

pub struct AgentTools<'a> {
    pub tool_runner: &'a dyn ToolRunner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[async_trait]
pub trait ContextManager: Send + Sync {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow>;
}

#[derive(Debug, Default)]
pub struct PassThroughContextManager;

#[async_trait]
impl ContextManager for PassThroughContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        Ok(ContextWindow {
            messages: messages.to_vec(),
            tools: agent.tools.clone(),
        })
    }
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
    Execution { result: String, duration_ms: u128 },
    Error { message: String },
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
            Err(err) => {
                return finish_session(session, SessionResult::SessionError(err.to_string())).await;
            }
        };
        let has_unprocessed_user_messages = match session.has_unprocessed_user_messages().await {
            Ok(value) => value,
            Err(err) => {
                return finish_session(session, SessionResult::SessionError(err.to_string())).await;
            }
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
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
                    }
                }

                let context = match session.build_context().await {
                    Ok(context) => context,
                    Err(err) => {
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
                    }
                };
                let req = LlmRequest {
                    model: self.model.clone(),
                    messages: to_provider_messages(&context.messages),
                    tools: to_provider_tool_specs(&context.tools),
                    temperature: None,
                    max_tokens: None,
                    api_key: None,
                };
                let assistant_message = match provider.chat(&req).await {
                    Ok(message) => message,
                    Err(err) => {
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
                    }
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
                    if matches!(
                        assistant_message.stop_reason,
                        StopReason::Aborted | StopReason::Error
                    ) {
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
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
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
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
                    }

                    info!(target: "borg_agent", session_id = session.session_id, tool_name, "tool_execution_start");
                    let output = match call_tool(tools, &tool_call_id, &tool_name, &arguments).await
                    {
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
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
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
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
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
    context_manager: Arc<dyn ContextManager>,
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
            context_manager: Arc::new(PassThroughContextManager),
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

    pub fn set_context_manager(&mut self, context_manager: Arc<dyn ContextManager>) {
        self.context_manager = context_manager;
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

    pub async fn build_context(&self) -> Result<ContextWindow> {
        let messages = self.read_messages(0, usize::MAX).await?;
        self.context_manager
            .build_context(&self.agent, &messages)
            .await
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
            Message::System { content } => Some(ProviderMessage::System {
                text: content.clone(),
            }),
            Message::User { content } => Some(ProviderMessage::User {
                content: vec![UserBlock::Text(content.clone())],
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
mod tests;
