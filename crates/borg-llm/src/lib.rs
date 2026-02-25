use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::info;

const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MAX_TURNS: usize = 6;

pub mod providers;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, model: &str, messages: &[Value], tools: &[Value]) -> Result<Value>;
}

pub struct LlmTools<'a> {
    pub execute: Box<dyn Fn(&str) -> Result<Value> + Send + Sync + 'a>,
    pub search: Box<dyn Fn(&str) -> Result<Value> + Send + Sync + 'a>,
}

#[derive(Debug, Clone)]
pub struct LlmSessionArgs {
    pub session_id: String,
    pub model: String,
    pub max_turns: usize,
}

impl LlmSessionArgs {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            model: DEFAULT_OPENAI_MODEL.to_string(),
            max_turns: DEFAULT_MAX_TURNS,
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
pub struct LlmSessionOutput {
    pub reply: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSession {
    pub session_id: String,
    pub model: String,
    pub max_turns: usize,
    pub messages: Vec<Message>,
}

impl LlmSession {
    pub fn new(args: LlmSessionArgs) -> Self {
        Self {
            session_id: args.session_id,
            model: args.model,
            max_turns: args.max_turns,
            messages: Vec::new(),
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

    pub async fn run<'a>(&mut self, api_key: &str, tools: &LlmTools<'a>) -> Result<LlmSessionOutput> {
        let provider = providers::openai::OpenAiProvider::new(api_key);
        self.run_with_provider(&provider, tools).await
    }

    pub async fn run_with_provider<'a, P: Provider>(
        &mut self,
        provider: &P,
        tools: &LlmTools<'a>,
    ) -> Result<LlmSessionOutput> {
        let mut records: Vec<ToolCallRecord> = Vec::new();
        let tool_specs = default_tool_specs();

        for turn in 0..self.max_turns {
            info!(target: "borg_llm", session_id = self.session_id, turn, "running llm session turn");
            let api_messages = to_openai_messages(&self.messages);
            let assistant_message = provider.chat(&self.model, &api_messages, &tool_specs).await?;

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
                return Ok(LlmSessionOutput {
                    reply,
                    tool_calls: records,
                });
            }

            for tool_call in tool_calls {
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

                self.add_message(Message::ToolCall {
                    tool_call_id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    arguments: arguments.clone(),
                });

                let output = call_tool(tools, &tool_name, &arguments)?;
                self.add_message(Message::ToolResult {
                    tool_call_id: tool_call_id.clone(),
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

        Err(anyhow!("llm session exceeded maximum turns"))
    }
}

fn default_tool_specs() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search capabilities or memory context for the user request",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "execute",
                "description": "Execute runtime code/action for task fulfillment",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string" }
                    },
                    "required": ["code"],
                    "additionalProperties": false
                }
            }
        }),
    ]
}

fn to_openai_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| match message {
            Message::System { content } => json!({
                "role": "system",
                "content": content
            }),
            Message::User { content } => json!({
                "role": "user",
                "content": content
            }),
            Message::Assistant { content } => json!({
                "role": "assistant",
                "content": content
            }),
            Message::ToolCall {
                tool_call_id,
                name,
                arguments,
            } => json!({
                "role": "assistant",
                "tool_calls": [
                    {
                        "id": tool_call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments.to_string()
                        }
                    }
                ]
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

fn call_tool<'a>(tools: &LlmTools<'a>, tool_name: &str, arguments: &Value) -> Result<Value> {
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
    tools: &LlmTools<'a>,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value> {
    call_tool(tools, tool_name, arguments)
}

#[cfg(test)]
mod tests {
    use super::{LlmSession, LlmSessionArgs, LlmTools, Message, call_tool_for_testing};
    use anyhow::anyhow;
    use serde_json::json;

    #[test]
    fn read_messages_respects_from_and_limit() {
        let mut session = LlmSession::new(LlmSessionArgs::new("s1"));
        session.add_message(Message::System {
            content: "s".to_string(),
        });
        session.add_message(Message::User {
            content: "u".to_string(),
        });
        session.add_message(Message::Assistant {
            content: "a".to_string(),
        });

        let got = session.read_messages(1, 1);
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0], Message::User { .. }));
    }

    #[test]
    fn injected_search_callback_can_return_empty_results() {
        let tools = LlmTools {
            execute: Box::new(|_| Ok(json!({ "ok": true }))),
            search: Box::new(|_| Ok(json!([]))),
        };
        let out = call_tool_for_testing(&tools, "search", &json!({"query": "x"})).unwrap();
        assert_eq!(out, json!([]));
    }

    #[test]
    fn injected_execute_callback_propagates_errors() {
        let tools = LlmTools {
            execute: Box::new(|_| Err(anyhow!("execution failed"))),
            search: Box::new(|_| Ok(json!([]))),
        };
        let err = call_tool_for_testing(&tools, "execute", &json!({"code": "1+1"}))
            .expect_err("expected execution failure");
        assert!(err.to_string().contains("execution failed"));
    }
}
