use anyhow::{Result, anyhow};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_llm::{LlmRequest, Provider, ProviderBlock, StopReason};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::{
    AgentTools, Session, SessionOutput, SessionResult, ToolCallRecord, ToolResultData, ToolSpec,
    call_tool, to_provider_messages, to_provider_tool_specs,
};

pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_MAX_TURNS: usize = 6;
pub const DEFAULT_AGENT_ID: &str = "borg:agent:default";
pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are Borg's agent runtime. Use tools as needed, then respond clearly.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: Uri,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tools: Vec<ToolSpec>,
}

impl Agent {
    pub fn new(agent_id: Uri) -> Self {
        Self {
            agent_id,
            model: DEFAULT_MODEL.to_string(),
            system_prompt: String::new(),
            max_turns: DEFAULT_MAX_TURNS,
            tools: Vec::new(),
        }
    }

    pub async fn load(agent_id: &Uri, db: &BorgDb) -> Result<Self> {
        if let Some(spec) = db.get_agent_spec(agent_id).await? {
            let tools: Vec<ToolSpec> = serde_json::from_value(spec.tools)?;
            return Ok(Self::new(spec.agent_id)
                .with_model(spec.model)
                .with_system_prompt(spec.system_prompt)
                .with_tools(tools));
        }

        if agent_id.as_str() == DEFAULT_AGENT_ID {
            return Ok(Self::new(agent_id.clone()).with_system_prompt(DEFAULT_SYSTEM_PROMPT));
        }

        Err(anyhow!("agent not found: {}", agent_id))
    }

    pub async fn default(db: &BorgDb) -> Result<Self> {
        let agent_id = uri!("borg", "agent", "default");
        Self::load(&agent_id, db).await
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
                let provider_messages = match to_provider_messages(&context.messages) {
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
                info!(target: "borg_agent", session_id = %session.session_id, "turn_end");

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
                    let output = match call_tool(tools, &tool_call_id, &tool_name, &arguments).await
                    {
                        Ok(value) => value,
                        Err(err) => ToolResultData::Error {
                            message: err.to_string(),
                        },
                    };
                    info!(target: "borg_agent", session_id = %session.session_id, tool_name, "tool_execution_end");

                    if let Err(err) = session
                        .add_message(crate::Message::ToolResult {
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

async fn finish_session(
    session: &mut Session,
    result: SessionResult<SessionOutput>,
) -> SessionResult<SessionOutput> {
    if let Err(err) = session.agent_finished(&result).await {
        return SessionResult::SessionError(err.to_string());
    }
    result
}
