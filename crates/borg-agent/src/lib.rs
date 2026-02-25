use anyhow::{Result, anyhow};
use borg_db::BorgDb;
use borg_llm::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::info;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TURNS: usize = 6;
const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";

pub struct AgentTools<'a> {
    pub execute: Box<dyn Fn(&str) -> Result<Value> + Send + Sync + 'a>,
    pub search: Box<dyn Fn(&str) -> Result<Value> + Send + Sync + 'a>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
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
                let assistant_message = match provider
                    .chat(
                        &self.model,
                        &to_provider_messages(&messages),
                        &to_provider_tool_specs(&self.tools),
                    )
                    .await
                {
                    Ok(message) => message,
                    Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
                };
                info!(target: "borg_agent", session_id = session.session_id, "turn_end");

                let tool_calls = assistant_message
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                if tool_calls.is_empty() {
                    last_reply = assistant_message
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
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
                for tool_call in tool_calls {
                    let (tool_call_id, tool_name, arguments) = match parse_tool_call(&tool_call) {
                        Ok(values) => values,
                        Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
                    };

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
                    let output = match call_tool(tools, &tool_name, &arguments) {
                        Ok(value) => value,
                        Err(err) => return finish_session(session, SessionResult::SessionError(err.to_string())).await,
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

                    if let Some(task_ids) = awaiting_task_ids(&output) {
                        if let Err(err) = session.mark_processed().await {
                            return finish_session(session, SessionResult::SessionError(err.to_string())).await;
                        }
                        return finish_session(session, SessionResult::Awaiting(task_ids)).await;
                    }

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
        content: Value,
    },
    SessionEvent {
        name: String,
        payload: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: Value,
    pub output: Value,
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
    Awaiting(Vec<String>),
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

    pub async fn mark_processed(&mut self) -> Result<()> {
        self.last_processed_len = self.db.count_session_messages(&self.session_id).await?;
        Ok(())
    }

    pub async fn agent_started(&mut self) -> Result<()> {
        self.add_message(Message::SessionEvent {
            name: AGENT_STARTED_EVENT.to_string(),
            payload: json!({}),
        })
        .await
    }

    pub async fn agent_finished(&mut self, result: &SessionResult<SessionOutput>) -> Result<()> {
        let payload = match result {
            SessionResult::Completed(Ok(output)) => {
                json!({ "status": "completed", "reply": output.reply })
            }
            SessionResult::Completed(Err(err)) => {
                json!({ "status": "completed_error", "error": err })
            }
            SessionResult::SessionError(err) => json!({ "status": "session_error", "error": err }),
            SessionResult::Awaiting(task_ids) => json!({ "status": "awaiting", "task_ids": task_ids }),
            SessionResult::Idle => json!({ "status": "idle" }),
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

fn to_provider_tool_specs(tool_specs: &[ToolSpec]) -> Vec<Value> {
    tool_specs
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters
                }
            })
        })
        .collect()
}

fn to_provider_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::System { content } => Some(json!({ "role": "system", "content": content })),
            Message::User { content } => Some(json!({ "role": "user", "content": content })),
            Message::Assistant { content } => Some(json!({ "role": "assistant", "content": content })),
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => Some(json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": tool_call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments.to_string()
                    }
                }]
            })),
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => Some(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "name": name,
                "content": content.to_string()
            })),
            Message::SessionEvent { .. } => None,
        })
        .collect()
}

fn parse_tool_call(tool_call: &Value) -> Result<(String, String, Value)> {
    let tool_call_id = tool_call
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing tool_call.id"))?
        .to_string();
    let function = tool_call
        .get("function")
        .ok_or_else(|| anyhow!("missing tool_call.function"))?;
    let tool_name = function
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing tool name"))?
        .to_string();
    let raw_arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let arguments: Value = serde_json::from_str(raw_arguments)?;
    Ok((tool_call_id, tool_name, arguments))
}

fn awaiting_task_ids(output: &Value) -> Option<Vec<String>> {
    let ids = output.get("awaiting_task_ids")?.as_array()?;
    let parsed: Vec<String> = ids
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

fn call_tool<'a>(tools: &AgentTools<'a>, tool_name: &str, arguments: &Value) -> Result<Value> {
    match tool_name {
        "execute" => {
            let code = arguments
                .get("code")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("execute tool requires code"))?;
            (tools.execute)(code)
        }
        "search" => {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("search tool requires query"))?;
            (tools.search)(query)
        }
        _ => Ok(json!({ "error": format!("unknown tool {}", tool_name) })),
    }
}

pub fn call_tool_for_testing<'a>(
    tools: &AgentTools<'a>,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value> {
    call_tool(tools, tool_name, arguments)
}

#[cfg(test)]
mod tests {
    use super::{AgentTools, SessionResult, call_tool_for_testing};
    use anyhow::anyhow;
    use serde_json::json;

    #[test]
    fn injected_search_callback_can_return_empty_results() {
        let tools = AgentTools {
            execute: Box::new(|_| Ok(json!({ "ok": true }))),
            search: Box::new(|_| Ok(json!([]))),
        };
        let out = call_tool_for_testing(&tools, "search", &json!({"query": "x"})).unwrap();
        assert_eq!(out, json!([]));
    }

    #[test]
    fn injected_execute_callback_propagates_errors() {
        let tools = AgentTools {
            execute: Box::new(|_| Err(anyhow!("execution failed"))),
            search: Box::new(|_| Ok(json!([]))),
        };
        let err = call_tool_for_testing(&tools, "execute", &json!({"code": "1+1"}))
            .expect_err("expected execution failure");
        assert!(err.to_string().contains("execution failed"));
    }

    #[test]
    fn session_result_idle_is_constructible() {
        let result: SessionResult<()> = SessionResult::Idle;
        assert!(matches!(result, SessionResult::Idle));
    }
}
