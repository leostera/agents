use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use borg_llm::completion::{
    CompletionRequest, CompletionResponse, FinishReason, InputItem, ModelSelector, OutputContent,
    OutputItem, TokenLimit, TopK, TopP,
};
use borg_llm::runner::LlmRunner;
use borg_llm::{completion::Temperature, completion::ToolChoice};
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone)]
pub enum AgentInput<M> {
    Message(M),
    Cancel,
}

#[derive(Debug, Clone)]
pub struct ExecutionProfile {
    pub model_selector: ModelSelector,
    pub temperature: Temperature,
    pub top_p: TopP,
    pub top_k: TopK,
    pub token_limit: TokenLimit,
    pub tool_choice: ToolChoice,
}

impl Default for ExecutionProfile {
    fn default() -> Self {
        Self {
            model_selector: ModelSelector::Any,
            temperature: Temperature::ProviderDefault,
            top_p: TopP::ProviderDefault,
            top_k: TopK::ProviderDefault,
            token_limit: TokenLimit::ProviderDefault,
            tool_choice: ToolChoice::ProviderDefault,
        }
    }
}

impl ExecutionProfile {
    pub fn deterministic() -> Self {
        Self {
            temperature: Temperature::Value(0.0),
            ..Self::default()
        }
    }

    pub fn volatile() -> Self {
        Self {
            temperature: Temperature::Value(1.0),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    InputAccepted {
        item: InputItem,
    },
    TurnStarted {
        turn: u64,
    },
    ModelOutputItem {
        item: OutputItem<(), String>,
    },
    TurnCompleted {
        turn: u64,
        finish_reason: FinishReason,
    },
    Completed {
        reply: String,
    },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum TurnOutcome<R> {
    Completed { reply: R },
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TurnReport<R> {
    pub turn: u64,
    pub events: Vec<AgentEvent>,
    pub outcome: TurnOutcome<R>,
}

#[async_trait]
pub trait AgentLlmClient: Send + Sync {
    async fn chat_string(
        &self,
        request: CompletionRequest<(), String>,
    ) -> AgentResult<CompletionResponse<(), String>>;
}

#[async_trait]
impl AgentLlmClient for LlmRunner {
    async fn chat_string(
        &self,
        request: CompletionRequest<(), String>,
    ) -> AgentResult<CompletionResponse<(), String>> {
        self.chat(request).await.map_err(AgentError::from)
    }
}

pub struct AgentBuilder {
    llm: Option<Arc<dyn AgentLlmClient>>,
    execution_profile: ExecutionProfile,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            llm: None,
            execution_profile: ExecutionProfile::default(),
        }
    }

    pub fn with_llm_runner<L>(mut self, llm: L) -> Self
    where
        L: AgentLlmClient + 'static,
    {
        self.llm = Some(Arc::new(llm));
        self
    }

    pub fn with_execution_profile(mut self, execution_profile: ExecutionProfile) -> Self {
        self.execution_profile = execution_profile;
        self
    }

    pub fn build(self) -> AgentResult<Agent> {
        let llm = self.llm.ok_or(AgentError::Internal {
            message: "AgentBuilder requires an llm runner".to_string(),
        })?;

        Ok(Agent {
            llm,
            execution_profile: self.execution_profile,
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        })
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Agent {
    llm: Arc<dyn AgentLlmClient>,
    execution_profile: ExecutionProfile,
    transcript: Mutex<Vec<InputItem>>,
    next_turn: AtomicU64,
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    pub async fn send<M>(&self, input: AgentInput<M>) -> AgentResult<TurnReport<String>>
    where
        M: Into<InputItem> + Send,
    {
        self.run_turn(input, None).await
    }

    pub async fn send_with_profile<M>(
        &self,
        input: AgentInput<M>,
        profile: ExecutionProfile,
    ) -> AgentResult<TurnReport<String>>
    where
        M: Into<InputItem> + Send,
    {
        self.run_turn(input, Some(profile)).await
    }

    pub async fn run_turn<M>(
        &self,
        input: AgentInput<M>,
        profile_override: Option<ExecutionProfile>,
    ) -> AgentResult<TurnReport<String>>
    where
        M: Into<InputItem> + Send,
    {
        let turn = self.next_turn.fetch_add(1, Ordering::SeqCst);
        let mut events = vec![AgentEvent::TurnStarted { turn }];

        match input {
            AgentInput::Cancel => {
                events.push(AgentEvent::Cancelled);
                Ok(TurnReport {
                    turn,
                    events,
                    outcome: TurnOutcome::Cancelled,
                })
            }
            AgentInput::Message(message) => {
                let item = message.into();
                events.push(AgentEvent::InputAccepted { item: item.clone() });

                let request = {
                    let mut transcript = self.transcript.lock().await;
                    transcript.push(item);

                    let profile =
                        profile_override.unwrap_or_else(|| self.execution_profile.clone());
                    build_request(transcript.clone(), &profile)
                };

                let response = self.llm.chat_string(request).await?;
                let reply = extract_reply(&response)?;

                {
                    let mut transcript = self.transcript.lock().await;
                    transcript.push(InputItem::assistant_text(reply.clone()));
                }

                for item in response.output.iter().cloned() {
                    events.push(AgentEvent::ModelOutputItem { item });
                }

                events.push(AgentEvent::TurnCompleted {
                    turn,
                    finish_reason: response.finish_reason.clone(),
                });
                events.push(AgentEvent::Completed {
                    reply: reply.clone(),
                });

                Ok(TurnReport {
                    turn,
                    events,
                    outcome: TurnOutcome::Completed { reply },
                })
            }
        }
    }

    pub async fn transcript(&self) -> Vec<InputItem> {
        self.transcript.lock().await.clone()
    }
}

fn build_request(
    input: Vec<InputItem>,
    profile: &ExecutionProfile,
) -> CompletionRequest<(), String> {
    let mut request = CompletionRequest::new(input, profile.model_selector.clone())
        .with_token_limit(profile.token_limit)
        .with_tool_choice(profile.tool_choice.clone());

    if let Temperature::Value(value) = profile.temperature {
        request = request.with_temperature(value);
    }

    if let TopP::Value(value) = profile.top_p {
        request = request.with_top_p(value);
    }

    if let TopK::Value(value) = profile.top_k {
        request = request.with_top_k(value);
    }

    request
}

fn extract_reply(response: &CompletionResponse<(), String>) -> AgentResult<String> {
    let reply = response
        .output
        .iter()
        .filter_map(|item| match item {
            OutputItem::Message {
                role: borg_llm::completion::Role::Assistant,
                content,
            } => Some(
                content
                    .iter()
                    .filter_map(|content| match content {
                        OutputContent::Text { text } => Some(text.as_str()),
                        OutputContent::Structured { .. } => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .find(|reply| !reply.trim().is_empty());

    reply.ok_or(AgentError::InvalidResponse {
        reason: "model returned no assistant text reply".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_llm::completion::{FinishReason, Role, Usage};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeLlmClient {
        responses: Mutex<VecDeque<AgentResult<CompletionResponse<(), String>>>>,
        requests: Mutex<Vec<CompletionRequest<(), String>>>,
    }

    impl FakeLlmClient {
        fn with_response(response: CompletionResponse<(), String>) -> Self {
            let mut responses = VecDeque::new();
            responses.push_back(Ok(response));
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }

        async fn take_requests(&self) -> Vec<CompletionRequest<(), String>> {
            self.requests.lock().await.clone()
        }
    }

    #[async_trait]
    impl AgentLlmClient for FakeLlmClient {
        async fn chat_string(
            &self,
            request: CompletionRequest<(), String>,
        ) -> AgentResult<CompletionResponse<(), String>> {
            self.requests.lock().await.push(request);
            self.responses.lock().await.pop_front().unwrap_or_else(|| {
                Err(AgentError::Internal {
                    message: "no fake response queued".to_string(),
                })
            })
        }
    }

    fn assistant_response(text: &str) -> CompletionResponse<(), String> {
        CompletionResponse {
            provider: borg_llm::completion::ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![OutputItem::Message {
                role: Role::Assistant,
                content: vec![OutputContent::Text {
                    text: text.to_string(),
                }],
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        }
    }

    fn provider_error() -> AgentError {
        AgentError::Llm(borg_llm::error::Error::Provider {
            provider: "openrouter".to_string(),
            status: 503,
            message: "temporarily unavailable".to_string(),
        })
    }

    #[tokio::test]
    async fn builder_errors_without_llm_runner() {
        let result = Agent::builder().build();
        assert!(matches!(result, Err(AgentError::Internal { .. })));
    }

    #[tokio::test]
    async fn send_records_input_and_reply_in_transcript() {
        let agent = Agent::builder()
            .with_llm_runner(FakeLlmClient::with_response(assistant_response(
                "hello back",
            )))
            .build()
            .expect("agent");

        let report = agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        assert!(matches!(
            report.outcome,
            TurnOutcome::Completed { ref reply } if reply == "hello back"
        ));

        let transcript = agent.transcript().await;
        assert_eq!(transcript.len(), 2);
        assert!(matches!(
            transcript[0],
            InputItem::Message {
                role: Role::User,
                ..
            }
        ));
        assert!(matches!(
            transcript[1],
            InputItem::Message {
                role: Role::Assistant,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn cancel_does_not_call_llm() {
        let fake = FakeLlmClient::default();
        let agent = Agent::builder()
            .with_llm_runner(fake)
            .build()
            .expect("agent");

        let report = agent
            .send::<InputItem>(AgentInput::Cancel)
            .await
            .expect("turn");

        assert!(matches!(report.outcome, TurnOutcome::Cancelled));
        assert!(matches!(report.events.last(), Some(AgentEvent::Cancelled)));
    }

    #[tokio::test]
    async fn send_reuses_prior_transcript_in_next_request() {
        let fake = Arc::new(FakeLlmClient {
            responses: Mutex::new(VecDeque::from(vec![
                Ok(assistant_response("first reply")),
                Ok(assistant_response("second reply")),
            ])),
            requests: Mutex::new(Vec::new()),
        });

        let agent = Agent {
            llm: fake.clone(),
            execution_profile: ExecutionProfile::default(),
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        };

        agent
            .send(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first turn");
        agent
            .send(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect("second turn");

        let requests = fake.take_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].input.len(), 1);
        assert_eq!(requests[1].input.len(), 3);
        assert!(matches!(
            requests[1].input[0],
            InputItem::Message {
                role: Role::User,
                ..
            }
        ));
        assert!(matches!(
            requests[1].input[1],
            InputItem::Message {
                role: Role::Assistant,
                ..
            }
        ));
        assert!(matches!(
            requests[1].input[2],
            InputItem::Message {
                role: Role::User,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn run_turn_returns_error_when_model_returns_no_assistant_text() {
        let response = CompletionResponse {
            provider: borg_llm::completion::ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![OutputItem::Reasoning {
                text: "thinking".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        };

        let agent = Agent::builder()
            .with_llm_runner(FakeLlmClient::with_response(response))
            .build()
            .expect("agent");

        let error = agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AgentError::InvalidResponse { .. }));
    }

    #[tokio::test]
    async fn send_applies_profile_override_to_request() {
        let fake = Arc::new(FakeLlmClient::with_response(assistant_response(
            "hello back",
        )));
        let agent = Agent {
            llm: fake.clone(),
            execution_profile: ExecutionProfile::default(),
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        };

        let profile = ExecutionProfile {
            model_selector: ModelSelector::from_model("override-model"),
            token_limit: TokenLimit::Max(42),
            temperature: Temperature::Value(0.0),
            top_p: TopP::ProviderDefault,
            top_k: TopK::ProviderDefault,
            tool_choice: ToolChoice::ProviderDefault,
        };

        agent
            .send_with_profile(AgentInput::Message(InputItem::user_text("hello")), profile)
            .await
            .expect("turn");

        let requests = fake.take_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(matches!(
            requests[0].model,
            ModelSelector::Specific { ref model, .. } if model == "override-model"
        ));
        assert_eq!(requests[0].token_limit, TokenLimit::Max(42));
        assert_eq!(requests[0].temperature, Temperature::Value(0.0));
    }

    #[tokio::test]
    async fn turn_events_include_model_output_and_completion() {
        let agent = Agent::builder()
            .with_llm_runner(FakeLlmClient::with_response(assistant_response(
                "hello back",
            )))
            .build()
            .expect("agent");

        let report = agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        assert!(
            report
                .events
                .iter()
                .any(|event| matches!(event, AgentEvent::ModelOutputItem { .. }))
        );
        assert!(report.events.iter().any(
            |event| matches!(event, AgentEvent::Completed { reply } if reply == "hello back")
        ));
    }

    #[tokio::test]
    async fn send_propagates_llm_errors() {
        let fake = FakeLlmClient {
            responses: Mutex::new(VecDeque::from(vec![Err(provider_error())])),
            requests: Mutex::new(Vec::new()),
        };
        let agent = Agent::builder()
            .with_llm_runner(fake)
            .build()
            .expect("agent");

        let error = agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect_err("should fail");

        match error {
            AgentError::Llm(inner) => {
                assert_eq!(inner.provider_name(), Some("openrouter"));
                assert_eq!(inner.provider_status(), Some(503));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
