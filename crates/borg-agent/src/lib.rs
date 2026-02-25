use anyhow::{Result, anyhow};
use borg_llm::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::info;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TURNS: usize = 6;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub agent: Agent,
    pub messages: Vec<Message>,
    last_processed_len: usize,
}

impl Session {
    pub fn new(session_id: impl Into<String>, agent: Agent) -> Self {
        let mut messages = Vec::new();
        if !agent.system_prompt.is_empty() {
            messages.push(Message::System {
                content: agent.system_prompt.clone(),
            });
        }
        Self {
            session_id: session_id.into(),
            agent,
            messages,
            last_processed_len: 0,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn read_messages(&self, from: usize, limit: usize) -> Vec<Message> {
        self.messages
            .iter()
            .skip(from)
            .take(limit)
            .cloned()
            .collect()
    }

    pub async fn run<'a, P: Provider>(
        &mut self,
        provider: &P,
        tools: &AgentTools<'a>,
    ) -> SessionResult<SessionOutput> {
        if self.messages.len() <= self.last_processed_len {
            return SessionResult::Idle;
        }

        let has_pending_user_message = self
            .messages
            .iter()
            .skip(self.last_processed_len)
            .any(|m| matches!(m, Message::User { .. }));
        if !has_pending_user_message {
            self.last_processed_len = self.messages.len();
            return SessionResult::Idle;
        }

        let mut records: Vec<ToolCallRecord> = Vec::new();
        for turn in 0..self.agent.max_turns {
            info!(target: "borg_agent", session_id = self.session_id, turn, "running session turn");
            let api_messages = to_provider_messages(&self.messages);
            let tool_specs = to_provider_tool_specs(&self.agent.tools);
            let assistant_message = match provider
                .chat(&self.agent.model, &api_messages, &tool_specs)
                .await
            {
                Ok(message) => message,
                Err(err) => return SessionResult::SessionError(err.to_string()),
            };

            let tool_calls = assistant_message
                .get("tool_calls")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if tool_calls.is_empty() {
                let reply = assistant_message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                self.add_message(Message::Assistant {
                    content: reply.clone(),
                });
                self.last_processed_len = self.messages.len();
                return SessionResult::Completed(Ok(SessionOutput {
                    reply,
                    tool_calls: records,
                }));
            }

            for tool_call in tool_calls {
                let parsed = parse_tool_call(&tool_call);
                let (tool_call_id, tool_name, arguments) = match parsed {
                    Ok(v) => v,
                    Err(err) => return SessionResult::SessionError(err.to_string()),
                };

                self.add_message(Message::ToolCall {
                    tool_call_id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    arguments: arguments.clone(),
                });

                let output = match call_tool(tools, &tool_name, &arguments) {
                    Ok(value) => value,
                    Err(err) => return SessionResult::SessionError(err.to_string()),
                };
                if let Some(task_ids) = awaiting_task_ids(&output) {
                    self.add_message(Message::ToolResult {
                        tool_call_id,
                        name: tool_name,
                        content: output,
                    });
                    self.last_processed_len = self.messages.len();
                    return SessionResult::Awaiting(task_ids);
                }

                self.add_message(Message::ToolResult {
                    tool_call_id,
                    name: tool_name.clone(),
                    content: output.clone(),
                });
                records.push(ToolCallRecord {
                    tool_name,
                    arguments,
                    output,
                });
            }
        }

        SessionResult::SessionError("agent session exceeded maximum turns".to_string())
    }
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
        .map(|message| match message {
            Message::System { content } => json!({ "role": "system", "content": content }),
            Message::User { content } => json!({ "role": "user", "content": content }),
            Message::Assistant { content } => json!({ "role": "assistant", "content": content }),
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": tool_call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments.to_string()
                    }
                }]
            }),
            Message::ToolResult {
                tool_call_id,
                name,
                content,
            } => json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "name": name,
                "content": content.to_string()
            }),
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
    if parsed.is_empty() { None } else { Some(parsed) }
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
    use super::{Agent, AgentTools, Message, Session, SessionResult, call_tool_for_testing};
    use anyhow::anyhow;
    use serde_json::json;

    #[test]
    fn read_messages_respects_from_and_limit() {
        let agent = Agent::new("a1");
        let mut session = Session::new("s1", agent);
        session.add_message(Message::User {
            content: "u".to_string(),
        });
        session.add_message(Message::Assistant {
            content: "a".to_string(),
        });

        let got = session.read_messages(1, 1);
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0], Message::User { .. } | Message::Assistant { .. }));
    }

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
