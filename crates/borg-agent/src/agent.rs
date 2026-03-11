use std::any::TypeId;
use std::sync::atomic::{AtomicU64, Ordering};

use borg_llm::completion::{
    CompletionRequest, CompletionResponse, FinishReason, InputItem, ModelSelector, OutputContent,
    OutputItem, ResponseMode, TokenLimit, TopK, TopP,
};
use borg_llm::response::TypedResponse;
use borg_llm::runner::LlmRunner;
use borg_llm::{completion::Temperature, completion::ToolChoice};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
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
pub enum AgentEvent<R> {
    InputAccepted {
        item: InputItem,
    },
    TurnStarted {
        turn: u64,
    },
    ModelOutputItem {
        item: OutputItem<(), R>,
    },
    TurnCompleted {
        turn: u64,
        finish_reason: FinishReason,
    },
    Completed {
        reply: R,
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
    pub events: Vec<AgentEvent<R>>,
    pub outcome: TurnOutcome<R>,
}

pub struct AgentBuilder {
    llm: Option<LlmRunner>,
    execution_profile: ExecutionProfile,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            llm: None,
            execution_profile: ExecutionProfile::default(),
        }
    }

    pub fn with_llm_runner(mut self, llm: LlmRunner) -> Self {
        self.llm = Some(llm);
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
    llm: LlmRunner,
    execution_profile: ExecutionProfile,
    transcript: Mutex<Vec<InputItem>>,
    next_turn: AtomicU64,
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    pub async fn send<M, R>(&self, input: AgentInput<M>) -> AgentResult<TurnReport<R>>
    where
        M: Into<InputItem> + Send,
        R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
    {
        self.run_turn(input, None).await
    }

    pub async fn send_with_profile<M, R>(
        &self,
        input: AgentInput<M>,
        profile: ExecutionProfile,
    ) -> AgentResult<TurnReport<R>>
    where
        M: Into<InputItem> + Send,
        R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
    {
        self.run_turn(input, Some(profile)).await
    }

    pub async fn run_turn<M, R>(
        &self,
        input: AgentInput<M>,
        profile_override: Option<ExecutionProfile>,
    ) -> AgentResult<TurnReport<R>>
    where
        M: Into<InputItem> + Send,
        R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
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
                    build_request::<R>(transcript.clone(), &profile)
                };

                let response = self.llm.chat::<(), R>(request).await?;
                let reply = extract_reply(&response)?;

                {
                    let mut transcript = self.transcript.lock().await;
                    transcript.push(assistant_item_for_reply(&reply)?);
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

fn build_request<R>(input: Vec<InputItem>, profile: &ExecutionProfile) -> CompletionRequest<(), R>
where
    R: JsonSchema + 'static,
{
    let mut request = CompletionRequest::new(input, profile.model_selector.clone())
        .with_token_limit(profile.token_limit)
        .with_tool_choice(profile.tool_choice.clone())
        .with_response_mode(ResponseMode::Buffered);

    if let Temperature::Value(value) = profile.temperature {
        request = request.with_temperature(value);
    }

    if let TopP::Value(value) = profile.top_p {
        request = request.with_top_p(value);
    }

    if let TopK::Value(value) = profile.top_k {
        request = request.with_top_k(value);
    }

    if TypeId::of::<R>() != TypeId::of::<String>() {
        request = request.with_typed_response(TypedResponse::new("agent_response"));
    }

    request
}

fn extract_reply<R>(response: &CompletionResponse<(), R>) -> AgentResult<R>
where
    R: Clone + DeserializeOwned + JsonSchema + 'static,
{
    for item in &response.output {
        match item {
            OutputItem::Message { content, .. } => {
                for content in content {
                    match content {
                        OutputContent::Structured { value } => return Ok(value.clone()),
                        OutputContent::Text { text }
                            if TypeId::of::<R>() == TypeId::of::<String>() =>
                        {
                            let boxed: Box<dyn std::any::Any> = Box::new(text.clone());
                            return boxed.downcast::<R>().map(|value| *value).map_err(|_| {
                                AgentError::Internal {
                                    message: "failed to downcast string response".to_string(),
                                }
                            });
                        }
                        OutputContent::Text { .. } => {}
                    }
                }
            }
            OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => {}
        }
    }

    Err(AgentError::InvalidResponse {
        reason: "model returned no assistant reply matching expected response type".to_string(),
    })
}

fn assistant_item_for_reply<R>(reply: &R) -> AgentResult<InputItem>
where
    R: Serialize + 'static,
{
    if TypeId::of::<R>() == TypeId::of::<String>() {
        let value = serde_json::to_value(reply).map_err(|error| AgentError::Internal {
            message: error.to_string(),
        })?;
        let text = value.as_str().ok_or(AgentError::Internal {
            message: "string reply did not serialize as string".to_string(),
        })?;
        return Ok(InputItem::assistant_text(text));
    }

    let text = serde_json::to_string(reply).map_err(|error| AgentError::Internal {
        message: error.to_string(),
    })?;
    Ok(InputItem::assistant_text(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use borg_llm::capability::Capability;
    use borg_llm::completion::{ProviderType, RawCompletionRequest, RawCompletionResponse, Role};
    use borg_llm::error::{Error as LlmError, LlmResult};
    use borg_llm::model::Model;
    use borg_llm::provider::LlmProvider;
    use borg_llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};
    use serde::{Deserialize, Serialize};
    use std::collections::VecDeque;
    use std::sync::Arc;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    struct EchoResponse {
        value: String,
    }

    struct FakeProvider {
        responses: Mutex<VecDeque<LlmResult<RawCompletionResponse>>>,
        requests: Mutex<Vec<RawCompletionRequest>>,
    }

    impl FakeProvider {
        fn with_response(response: RawCompletionResponse) -> Self {
            let mut responses = VecDeque::new();
            responses.push_back(Ok(response));
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }

        async fn take_requests(&self) -> Vec<RawCompletionRequest> {
            self.requests.lock().await.clone()
        }
    }

    #[async_trait]
    impl LlmProvider for FakeProvider {
        fn provider_type(&self) -> ProviderType {
            ProviderType::OpenAI
        }

        fn provider_name(&self) -> &'static str {
            "fake"
        }

        fn capabilities(&self) -> &[Capability] {
            &[]
        }

        async fn available_models(&self) -> LlmResult<Vec<Model>> {
            Ok(Vec::new())
        }

        async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
            self.requests.lock().await.push(req);
            self.responses.lock().await.pop_front().unwrap_or_else(|| {
                Err(LlmError::Internal {
                    message: "no fake response queued".to_string(),
                })
            })
        }

        async fn transcribe(
            &self,
            _req: AudioTranscriptionRequest,
        ) -> LlmResult<AudioTranscriptionResponse> {
            Err(LlmError::InvalidRequest {
                reason: "unsupported".to_string(),
            })
        }
    }

    fn runner_with_provider(provider: FakeProvider) -> LlmRunner {
        LlmRunner::builder().add_provider(provider).build()
    }

    fn assistant_text_response(text: &str) -> RawCompletionResponse {
        RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![borg_llm::completion::RawOutputItem::Message {
                role: Role::Assistant,
                content: vec![borg_llm::completion::RawOutputContent::Text {
                    text: text.to_string(),
                }],
            }],
            usage: borg_llm::completion::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        }
    }

    fn assistant_json_response(value: serde_json::Value) -> RawCompletionResponse {
        RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![borg_llm::completion::RawOutputItem::Message {
                role: Role::Assistant,
                content: vec![borg_llm::completion::RawOutputContent::Json { value }],
            }],
            usage: borg_llm::completion::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        }
    }

    fn provider_error() -> LlmError {
        LlmError::Provider {
            provider: "openrouter".to_string(),
            status: 503,
            message: "temporarily unavailable".to_string(),
        }
    }

    #[tokio::test]
    async fn builder_errors_without_llm_runner() {
        let result = Agent::builder().build();
        assert!(matches!(result, Err(AgentError::Internal { .. })));
    }

    #[tokio::test]
    async fn send_records_string_input_and_reply_in_transcript() {
        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider::with_response(
                assistant_text_response("hello back"),
            )))
            .build()
            .expect("agent");

        let report = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
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
    async fn send_decodes_typed_response() {
        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider::with_response(
                assistant_json_response(serde_json::json!({ "value": "typed-ok" })),
            )))
            .build()
            .expect("agent");

        let report = agent
            .send::<_, EchoResponse>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        assert!(matches!(
            report.outcome,
            TurnOutcome::Completed { reply: EchoResponse { ref value } } if value == "typed-ok"
        ));
    }

    #[tokio::test]
    async fn cancel_does_not_call_llm() {
        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider {
                responses: Mutex::new(VecDeque::new()),
                requests: Mutex::new(Vec::new()),
            }))
            .build()
            .expect("agent");

        let report = agent
            .send::<InputItem, String>(AgentInput::Cancel)
            .await
            .expect("turn");

        assert!(matches!(report.outcome, TurnOutcome::Cancelled));
        assert!(matches!(report.events.last(), Some(AgentEvent::Cancelled)));
    }

    #[tokio::test]
    async fn send_reuses_prior_transcript_in_next_request() {
        let fake = Arc::new(FakeProvider {
            responses: Mutex::new(VecDeque::from(vec![
                Ok(assistant_text_response("first reply")),
                Ok(assistant_text_response("second reply")),
            ])),
            requests: Mutex::new(Vec::new()),
        });

        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(fake.clone()))
            .build();
        let agent = Agent {
            llm: runner,
            execution_profile: ExecutionProfile::default(),
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        };

        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first turn");
        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect("second turn");

        let requests = fake.take_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].input.len(), 1);
        assert_eq!(requests[1].input.len(), 3);
    }

    #[tokio::test]
    async fn run_turn_returns_error_when_model_returns_no_matching_reply_type() {
        let response = RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![borg_llm::completion::RawOutputItem::Reasoning {
                text: "thinking".to_string(),
            }],
            usage: borg_llm::completion::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        };

        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider::with_response(response)))
            .build()
            .expect("agent");

        let error = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AgentError::InvalidResponse { .. }));
    }

    #[tokio::test]
    async fn send_applies_profile_override_to_request() {
        let fake = Arc::new(FakeProvider::with_response(assistant_text_response(
            "hello back",
        )));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(fake.clone()))
            .build();
        let agent = Agent {
            llm: runner,
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
            .send_with_profile::<_, String>(
                AgentInput::Message(InputItem::user_text("hello")),
                profile,
            )
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
    async fn typed_send_sets_typed_response_format() {
        let fake = Arc::new(FakeProvider::with_response(assistant_json_response(
            serde_json::json!({ "value": "typed-ok" }),
        )));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(fake.clone()))
            .build();
        let agent = Agent {
            llm: runner,
            execution_profile: ExecutionProfile::default(),
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        };

        agent
            .send::<_, EchoResponse>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        let requests = fake.take_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].response_format.is_some());
    }

    #[tokio::test]
    async fn string_send_does_not_set_typed_response_format() {
        let fake = Arc::new(FakeProvider::with_response(assistant_text_response(
            "hello back",
        )));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(fake.clone()))
            .build();
        let agent = Agent {
            llm: runner,
            execution_profile: ExecutionProfile::default(),
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        };

        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        let requests = fake.take_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].response_format.is_none());
    }

    #[tokio::test]
    async fn turn_events_include_model_output_and_completion() {
        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider::with_response(
                assistant_text_response("hello back"),
            )))
            .build()
            .expect("agent");

        let report = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
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
        let agent = Agent::builder()
            .with_llm_runner(runner_with_provider(FakeProvider {
                responses: Mutex::new(VecDeque::from(vec![Err(provider_error())])),
                requests: Mutex::new(Vec::new()),
            }))
            .build()
            .expect("agent");

        let error = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
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

    struct ArcBackedFakeProvider(Arc<FakeProvider>);

    #[async_trait]
    impl LlmProvider for ArcBackedFakeProvider {
        fn provider_type(&self) -> ProviderType {
            self.0.provider_type()
        }

        fn provider_name(&self) -> &'static str {
            self.0.provider_name()
        }

        fn capabilities(&self) -> &[Capability] {
            self.0.capabilities()
        }

        async fn available_models(&self) -> LlmResult<Vec<Model>> {
            self.0.available_models().await
        }

        async fn chat_raw(&self, req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
            self.0.chat_raw(req).await
        }

        async fn transcribe(
            &self,
            req: AudioTranscriptionRequest,
        ) -> LlmResult<AudioTranscriptionResponse> {
            self.0.transcribe(req).await
        }
    }
}
