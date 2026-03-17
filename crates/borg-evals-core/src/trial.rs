use borg_agent::{AgentEvent, ToolExecutionResult};
use borg_llm::completion::{OutputContent, OutputItem, Role};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::grade::{GradeResult, GraderFailure};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentTrial<Output = String> {
    pub transcript: Vec<RecordedEvent>,
    pub final_reply: Option<Output>,
    pub tool_trace: Vec<RecordedToolCall>,
    #[serde(default)]
    pub grades: Vec<GradeResult>,
    #[serde(default)]
    pub grader_failures: Vec<GraderFailure>,
    #[serde(default)]
    pub metadata: Value,
}

impl<Output> AgentTrial<Output> {
    pub fn new(final_reply: Output) -> Self {
        Self {
            transcript: Vec::new(),
            final_reply: Some(final_reply),
            tool_trace: Vec::new(),
            grades: Vec::new(),
            grader_failures: Vec::new(),
            metadata: Value::Null,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentTrialRecorder<Output = String> {
    transcript: Vec<RecordedEvent>,
    final_reply: Option<Output>,
    tool_trace: Vec<RecordedToolCall>,
}

impl<Output> Default for AgentTrialRecorder<Output> {
    fn default() -> Self {
        Self {
            transcript: Vec::new(),
            final_reply: None,
            tool_trace: Vec::new(),
        }
    }
}

impl<Output> AgentTrialRecorder<Output> {
    pub fn record<Tool, ToolResult>(&mut self, event: &AgentEvent<Tool, ToolResult, Output>)
    where
        ToolResult: Serialize,
        Output: Clone + Serialize,
    {
        match event {
            AgentEvent::ModelOutputItem { item } => match item {
                OutputItem::Message { role, content } => {
                    let text = content
                        .iter()
                        .filter_map(|content| match content {
                            OutputContent::Text { text } => Some(text.clone()),
                            OutputContent::Structured { .. } => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim()
                        .to_string();

                    if !text.is_empty() {
                        self.transcript.push(RecordedEvent::Message {
                            role: match role {
                                Role::System => RecordedMessageRole::System,
                                Role::User => RecordedMessageRole::User,
                                Role::Assistant => RecordedMessageRole::Assistant,
                            },
                            content: text,
                        });
                    }
                }
                OutputItem::Reasoning { text } => {
                    if !text.trim().is_empty() {
                        self.transcript.push(RecordedEvent::Message {
                            role: RecordedMessageRole::Assistant,
                            content: text.trim().to_string(),
                        });
                    }
                }
                OutputItem::ToolCall { .. } => {}
            },
            AgentEvent::ToolCallRequested { call } => {
                let arguments = call.arguments.clone();
                self.transcript.push(RecordedEvent::ToolCallRequested {
                    id: call.call_id.clone(),
                    name: call.name.clone(),
                    arguments: arguments.clone(),
                });
                self.tool_trace.push(RecordedToolCall {
                    id: call.call_id.clone(),
                    name: call.name.clone(),
                    arguments,
                    result: None,
                    error: None,
                });
            }
            AgentEvent::ToolExecutionCompleted { result } => {
                let result_value = match &result.result {
                    ToolExecutionResult::Ok { data } => serde_json::to_value(data)
                        .expect("serialize tool result for trial recording"),
                    ToolExecutionResult::Error { message } => {
                        serde_json::json!({ "error": message })
                    }
                };
                self.transcript.push(RecordedEvent::ToolExecutionCompleted {
                    id: result.call_id.clone(),
                    name: self
                        .tool_trace
                        .iter()
                        .find(|tool| tool.id == result.call_id)
                        .map(|tool| tool.name.clone())
                        .unwrap_or_else(|| "unknown_tool".to_string()),
                    result: result_value.clone(),
                });
                if let Some(tool) = self
                    .tool_trace
                    .iter_mut()
                    .find(|tool| tool.id == result.call_id)
                {
                    match &result.result {
                        ToolExecutionResult::Ok { data } => {
                            tool.result = Some(
                                serde_json::to_value(data)
                                    .expect("serialize tool result for tool trace"),
                            )
                        }
                        ToolExecutionResult::Error { message } => {
                            tool.error = Some(message.clone())
                        }
                    }
                }
            }
            AgentEvent::Completed { reply } => {
                self.transcript.push(RecordedEvent::Completed {
                    reply: serde_json::to_value(reply).expect("serialize completed reply"),
                });
                self.final_reply = Some(reply.clone());
            }
            AgentEvent::Cancelled => {}
        }
    }

    pub fn final_reply(&self) -> Option<&Output> {
        self.final_reply.as_ref()
    }

    pub fn snapshot(&self, metadata: Value) -> AgentTrial<Output>
    where
        Output: Clone,
    {
        AgentTrial {
            transcript: self.transcript.clone(),
            final_reply: self.final_reply.clone(),
            tool_trace: self.tool_trace.clone(),
            grades: Vec::new(),
            grader_failures: Vec::new(),
            metadata,
        }
    }

    pub fn into_trial(self, metadata: Value) -> AgentTrial<Output> {
        AgentTrial {
            transcript: self.transcript,
            final_reply: self.final_reply,
            tool_trace: self.tool_trace,
            grades: Vec::new(),
            grader_failures: Vec::new(),
            metadata,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedEvent {
    Message {
        role: RecordedMessageRole,
        content: String,
    },
    ToolCallRequested {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolExecutionCompleted {
        id: String,
        name: String,
        result: Value,
    },
    Completed {
        reply: Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordedMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecordedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub result: Option<Value>,
    pub error: Option<String>,
}
