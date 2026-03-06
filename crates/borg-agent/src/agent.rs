use anyhow::{Result, anyhow};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_llm::{LlmRequest, Provider, ProviderBlock, ReasoningEffort, StopReason};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;
use tracing::{Instrument, error, info_span, warn};
use uuid::Uuid;

use crate::{
    Session, SessionOutput, SessionResult, ToolCallRecord, ToolRequest, ToolResultData, ToolSpec,
    Toolchain, to_provider_messages, to_provider_tool_specs,
};

pub const DEFAULT_MAX_TURNS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent<TToolCall, TToolResult> {
    pub actor_id: Uri,
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub system_prompt: String,
    pub behavior_prompt: String,
    pub max_turns: usize,
    pub tools: Vec<ToolSpec>,
    #[serde(skip)]
    _marker: PhantomData<(TToolCall, TToolResult)>,
}

impl<TToolCall, TToolResult> Agent<TToolCall, TToolResult> {
    pub fn new(actor_id: Uri) -> Self {
        Self {
            actor_id,
            model: String::new(),
            reasoning_effort: None,
            system_prompt: String::new(),
            behavior_prompt: String::new(),
            max_turns: DEFAULT_MAX_TURNS,
            tools: Vec::new(),
            _marker: PhantomData,
        }
    }

    pub async fn load(actor_id: &Uri, db: &BorgDb) -> Result<Self> {
        if let Some(actor) = db.get_actor(actor_id).await? {
            let model = actor
                .model
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "model not configured for actor {} (set one via /model <name>)",
                        actor.actor_id
                    )
                })?;
            return Ok(Self::new(actor.actor_id)
                .with_model(model)
                .with_system_prompt(actor.system_prompt));
        }

        Err(anyhow!("actor not found: {}", actor_id))
    }

    pub async fn default(db: &BorgDb) -> Result<Self> {
        let actor_id = uri!("borg", "actor", "default");
        Self::load(&actor_id, db).await
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = system_prompt.into();
        self
    }

    pub fn with_behavior_prompt(mut self, behavior_prompt: impl Into<String>) -> Self {
        self.behavior_prompt = behavior_prompt.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_reasoning_effort(mut self, reasoning_effort: Option<ReasoningEffort>) -> Self {
        self.reasoning_effort = reasoning_effort;
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

impl<TToolCall, TToolResult> Agent<TToolCall, TToolResult>
where
    TToolCall: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    TToolResult: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub async fn run<P: Provider>(
        &self,
        session: &mut Session<TToolCall, TToolResult>,
        provider: &P,
        tools: &Toolchain<TToolCall, TToolResult>,
    ) -> SessionResult<SessionOutput<TToolCall, TToolResult>> {
        self.run_with_tool_events(session, provider, tools, None)
            .await
    }

    pub async fn run_with_tool_events<P: Provider>(
        &self,
        session: &mut Session<TToolCall, TToolResult>,
        provider: &P,
        tools: &Toolchain<TToolCall, TToolResult>,
        tool_event_tx: Option<&UnboundedSender<ToolCallRecord<TToolCall, TToolResult>>>,
    ) -> SessionResult<SessionOutput<TToolCall, TToolResult>> {
        if let Err(err) = session.agent_started().await {
            return SessionResult::SessionError(err.to_string());
        }

        let mut pending = session.pop_steering_messages();
        let user_key = match session.user_key().await {
            Ok(value) => Some(value),
            Err(err) => {
                warn!(
                    target: "borg_agent",
                    session_id = %session.session_id,
                    error = %err,
                    "failed to resolve user_key for provider call context"
                );
                None
            }
        };
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
        let mut records: Vec<ToolCallRecord<TToolCall, TToolResult>> = Vec::new();

        while has_tool_calls || !pending.is_empty() {
            while has_tool_calls || !pending.is_empty() {
                info!(target: "borg_agent", session_id = %session.session_id, "turn_start");

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
                let provider_input_messages = context.provider_input_messages();
                let provider_messages = match to_provider_messages(&provider_input_messages) {
                    Ok(messages) => messages,
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
                    messages: provider_messages,
                    tools: to_provider_tool_specs(&context.available_tools),
                    temperature: None,
                    max_tokens: None,
                    reasoning_effort: self.reasoning_effort,
                    api_key: None,
                };
                let call_id = uri!("borg", "call").to_string();
                let llm_call_span = info_span!(
                    "llm_provider_call",
                    call_id = %call_id,
                    session_id = %session.session_id,
                    user_id = ?user_key.as_ref().map(Uri::to_string),
                    model = req.model.as_str()
                );
                let assistant_message = match provider.chat(&req).instrument(llm_call_span).await {
                    Ok(message) => message,
                    Err(err) => {
                        return finish_session(
                            session,
                            SessionResult::SessionError(err.to_string()),
                        )
                        .await;
                    }
                };
                if let Err(err) = session
                    .record_provider_usage(
                        provider.provider_name(),
                        assistant_message.usage_tokens.unwrap_or(0),
                    )
                    .await
                {
                    warn!(
                        target: "borg_agent",
                        session_id = %session.session_id,
                        provider = provider.provider_name(),
                        error = %err,
                        "failed to update provider usage summary"
                    );
                }
                info!(target: "borg_agent", session_id = %session.session_id, "turn_end");

                let mut tool_calls: Vec<(String, String, TToolCall)> = Vec::new();
                for block in &assistant_message.content {
                    let ProviderBlock::ToolCall {
                        id,
                        name,
                        arguments_json,
                    } = block
                    else {
                        continue;
                    };

                    let arguments =
                        match serde_json::from_value::<TToolCall>(arguments_json.clone()) {
                            Ok(value) => value,
                            Err(err) => {
                                return finish_session(
                                    session,
                                    SessionResult::SessionError(format!(
                                        "invalid tool call arguments for `{name}`: {err}"
                                    )),
                                )
                                .await;
                            }
                        };
                    tool_calls.push((id.clone(), name.clone(), arguments));
                }

                if tool_calls.is_empty() {
                    if matches!(
                        assistant_message.stop_reason,
                        StopReason::Aborted | StopReason::Error
                    ) {
                        let session_error_message = assistant_message
                            .error_message
                            .clone()
                            .unwrap_or_else(|| "assistant aborted or errored".to_string());
                        error!(
                            target: "borg_agent",
                            session_id = %session.session_id,
                            stop_reason = ?assistant_message.stop_reason,
                            block_count = assistant_message.content.len(),
                            error = session_error_message.as_str(),
                            "assistant turn ended with error stop reason"
                        );
                        return finish_session(
                            session,
                            SessionResult::SessionError(session_error_message),
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
                        .add_message(crate::Message::Assistant {
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
                        .add_message(crate::Message::ToolCall {
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

                    info!(target: "borg_agent", session_id = %session.session_id, tool_name, "tool_execution_start");
                    let output = match tools
                        .run(ToolRequest {
                            tool_call_id: tool_call_id.clone(),
                            tool_name: tool_name.clone(),
                            arguments: arguments.clone(),
                        })
                        .await
                    {
                        Ok(response) => response.content,
                        Err(err) => ToolResultData::Error {
                            message: err.to_string(),
                        },
                    };
                    info!(target: "borg_agent", session_id = %session.session_id, tool_name, "tool_execution_end");

                    if let Err(err) = session
                        .add_message(crate::Message::ToolResult {
                            tool_call_id: tool_call_id.clone(),
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

                    let record = ToolCallRecord {
                        tool_name: tool_name.clone(),
                        arguments: arguments.clone(),
                        output: output.clone(),
                    };
                    if let Some(tx) = tool_event_tx {
                        let _ = tx.send(record.clone());
                    }
                    records.push(record);

                    let persisted_call_id = format!("{tool_call_id}:{}", Uuid::now_v7());
                    if let Err(err) = session
                        .record_tool_call(&persisted_call_id, &tool_name, &arguments, &output)
                        .await
                    {
                        warn!(
                            target: "borg_agent",
                            session_id = %session.session_id,
                            call_id = %persisted_call_id,
                            tool_name = %tool_name,
                            error = %err,
                            "failed to persist tool call"
                        );
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

async fn finish_session<TToolCall, TToolResult>(
    session: &mut Session<TToolCall, TToolResult>,
    result: SessionResult<SessionOutput<TToolCall, TToolResult>>,
) -> SessionResult<SessionOutput<TToolCall, TToolResult>>
where
    TToolCall: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    TToolResult: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    if let Err(err) = session.agent_finished(&result).await {
        return SessionResult::SessionError(err.to_string());
    }
    result
}
