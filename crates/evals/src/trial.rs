use agents::agent::{Agent, AgentError, AgentEvent, AgentInput, ToolExecutionResult};
use agents::llm::completion::{OutputContent, OutputItem, Role};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::error::EvalError;
use crate::grade::{GradeResult, GraderFailure};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentTrial<Output = String> {
    pub transcript: Vec<RecordedEvent>,
    pub final_reply: Option<Output>,
    pub tool_trace: Vec<RecordedToolCall>,
    #[serde(default)]
    pub grades: BTreeMap<String, GradeResult>,
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
            grades: BTreeMap::new(),
            grader_failures: Vec::new(),
            metadata: Value::Null,
        }
    }

    pub fn from_transcript(
        transcript: Vec<RecordedEvent>,
        final_reply: Option<Output>,
        metadata: Value,
    ) -> Self {
        Self {
            tool_trace: build_tool_trace(&transcript),
            transcript,
            final_reply,
            grades: BTreeMap::new(),
            grader_failures: Vec::new(),
            metadata,
        }
    }
}

pub(crate) type TranscriptSender = mpsc::Sender<RecordedEvent>;

pub(crate) struct TranscriptCollector {
    receiver: mpsc::Receiver<RecordedEvent>,
    store: Vec<RecordedEvent>,
}

impl TranscriptCollector {
    pub(crate) fn new(capacity: usize) -> (TranscriptSender, Self) {
        let (tx, receiver) = mpsc::channel(capacity.max(1));
        (
            tx,
            Self {
                receiver,
                store: Vec::new(),
            },
        )
    }

    pub(crate) async fn snapshot(&mut self) -> Vec<RecordedEvent> {
        self.drain_pending();
        self.store.clone()
    }

    pub(crate) async fn finish(mut self) -> Vec<RecordedEvent> {
        self.drain_pending();
        while let Some(event) = self.receiver.recv().await {
            self.store.push(event);
        }
        self.store
    }

    fn drain_pending(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            self.store.push(event);
        }
    }
}

pub(crate) struct TranscriptAgent<A: Agent> {
    inner: A,
    transcript: TranscriptSender,
}

impl<A: Agent> TranscriptAgent<A> {
    pub(crate) fn new(inner: A, transcript: TranscriptSender) -> Self {
        Self { inner, transcript }
    }
}

#[async_trait]
impl<A> Agent for TranscriptAgent<A>
where
    A: Agent,
{
    type Input = A::Input;
    type ToolCall = A::ToolCall;
    type ToolResult = A::ToolResult;
    type Output = A::Output;

    async fn send(&mut self, input: AgentInput<Self::Input>) -> agents::agent::AgentResult<()> {
        self.inner.send(input.clone()).await?;
        if let Some(event) = recorded_event_from_agent_input(&input) {
            let _ = self.transcript.send(event).await;
        }
        Ok(())
    }

    async fn next(
        &mut self,
    ) -> agents::agent::AgentResult<
        Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>,
    > {
        let event = match self.inner.next().await {
            Ok(event) => event,
            Err(error) => {
                let _ = self
                    .transcript
                    .send(RecordedEvent::Error {
                        error: RecordedError::from_agent_error(&error),
                    })
                    .await;
                return Err(error);
            }
        };
        if let Some(event_ref) = event.as_ref() {
            for recorded in recorded_events_from_agent_event(event_ref) {
                let _ = self.transcript.send(recorded).await;
            }
        }
        Ok(event)
    }

    async fn spawn(
        self,
    ) -> agents::agent::AgentResult<(
        agents::agent::AgentRunInput<Self::Input>,
        agents::agent::AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        let transcript = self.transcript.clone();
        let (inner_input, mut inner_output) = self.inner.spawn().await?;
        let (input_tx, mut input_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    maybe_input = input_rx.recv() => {
                        match maybe_input {
                            Some(input) => {
                                if let Some(event) = recorded_event_from_agent_input(&input) {
                                    let _ = transcript.send(event).await;
                                }
                                if inner_input.send(input).await.is_err() {
                                    return;
                                }
                            }
                            None => return,
                        }
                    }
                    maybe_event = inner_output.recv() => {
                        match maybe_event {
                            Some(Ok(event)) => {
                                for recorded in recorded_events_from_agent_event(&event) {
                                    let _ = transcript.send(recorded).await;
                                }
                                if event_tx.send(Ok(event)).await.is_err() {
                                    return;
                                }
                            }
                            Some(Err(error)) => {
                                let _ = transcript
                                    .send(RecordedEvent::Error {
                                        error: RecordedError::from_agent_error(&error),
                                    })
                                    .await;
                                if event_tx.send(Err(error)).await.is_err() {
                                    return;
                                }
                            }
                            None => return,
                        }
                    }
                }
            }
        });

        Ok((input_tx, event_rx))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedEvent {
    StepStarted {
        step_index: usize,
        input: Value,
    },
    StepCompleted {
        step_index: usize,
    },
    Message {
        role: RecordedMessageRole,
        content: String,
    },
    Thinking {
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
    Error {
        error: RecordedError,
    },
    GraderStarted {
        scope: RecordedGradingScope,
        grader: String,
    },
    GraderCompleted {
        scope: RecordedGradingScope,
        grader: String,
        score: f32,
        summary: String,
        evidence: Value,
    },
    GraderFailed {
        scope: RecordedGradingScope,
        grader: String,
        error: RecordedError,
    },
}

#[derive(Clone, Debug, Error, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "error", rename_all = "snake_case")]
pub enum RecordedError {
    #[error(transparent)]
    AgentError(AgentError),
    #[error(transparent)]
    EvalError(EvalError),
}

impl RecordedError {
    pub(crate) fn from_agent_error(error: &AgentError) -> Self {
        Self::AgentError(error.clone())
    }

    pub(crate) fn from_eval_error(error: &EvalError) -> Self {
        Self::EvalError(error.clone())
    }

    pub(crate) fn eval_message(message: impl Into<String>) -> Self {
        Self::EvalError(EvalError::message(message))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum RecordedGradingScope {
    Eval,
    TrajectoryStep { step_index: usize },
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

fn recorded_event_from_agent_input<Input>(input: &AgentInput<Input>) -> Option<RecordedEvent>
where
    Input: Serialize,
{
    match input {
        AgentInput::Message(message) => Some(RecordedEvent::Message {
            role: RecordedMessageRole::User,
            content: serde_json::to_string(message).expect("serialize agent input"),
        }),
        AgentInput::Steer(message) => Some(RecordedEvent::Message {
            role: RecordedMessageRole::User,
            content: serde_json::to_string(message).expect("serialize steer input"),
        }),
        AgentInput::Cancel => None,
    }
}

fn recorded_events_from_agent_event<Tool, ToolResult, Output>(
    event: &AgentEvent<Tool, ToolResult, Output>,
) -> Vec<RecordedEvent>
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

                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![RecordedEvent::Message {
                        role: match role {
                            Role::System => RecordedMessageRole::System,
                            Role::User => RecordedMessageRole::User,
                            Role::Assistant => RecordedMessageRole::Assistant,
                        },
                        content: text,
                    }]
                }
            }
            OutputItem::Reasoning { text } => {
                if text.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![RecordedEvent::Thinking {
                        content: text.trim().to_string(),
                    }]
                }
            }
            OutputItem::ToolCall { .. } => Vec::new(),
        },
        AgentEvent::ToolCallRequested { call } => vec![RecordedEvent::ToolCallRequested {
            id: call.call_id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        }],
        AgentEvent::ToolExecutionCompleted { result } => {
            let result_value = match &result.result {
                ToolExecutionResult::Ok { data } => {
                    serde_json::to_value(data).expect("serialize tool result for transcript")
                }
                ToolExecutionResult::Error { message } => {
                    serde_json::json!({ "error": message })
                }
            };
            vec![RecordedEvent::ToolExecutionCompleted {
                id: result.call_id.clone(),
                name: "unknown_tool".to_string(),
                result: result_value,
            }]
        }
        AgentEvent::Completed { reply } => vec![RecordedEvent::Completed {
            reply: serde_json::to_value(reply).expect("serialize completed reply"),
        }],
        AgentEvent::Cancelled => Vec::new(),
    }
}

fn build_tool_trace(transcript: &[RecordedEvent]) -> Vec<RecordedToolCall> {
    let mut tool_trace = Vec::new();

    for event in transcript {
        match event {
            RecordedEvent::ToolCallRequested {
                id,
                name,
                arguments,
            } => {
                tool_trace.push(RecordedToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                    result: None,
                    error: None,
                });
            }
            RecordedEvent::ToolExecutionCompleted { id, name, result } => {
                if let Some(tool) = tool_trace.iter_mut().find(|tool| tool.id == *id) {
                    tool.name = name.clone();
                    if let Some(error) = result.get("error").and_then(Value::as_str) {
                        tool.error = Some(error.to_string());
                    } else {
                        tool.result = Some(result.clone());
                    }
                } else {
                    let mut tool = RecordedToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: Value::Null,
                        result: None,
                        error: None,
                    };
                    if let Some(error) = result.get("error").and_then(Value::as_str) {
                        tool.error = Some(error.to_string());
                    } else {
                        tool.result = Some(result.clone());
                    }
                    tool_trace.push(tool);
                }
            }
            RecordedEvent::StepStarted { .. }
            | RecordedEvent::StepCompleted { .. }
            | RecordedEvent::Message { .. }
            | RecordedEvent::Thinking { .. }
            | RecordedEvent::Completed { .. }
            | RecordedEvent::Error { .. }
            | RecordedEvent::GraderStarted { .. }
            | RecordedEvent::GraderCompleted { .. }
            | RecordedEvent::GraderFailed { .. } => {}
        }
    }

    tool_trace
}
