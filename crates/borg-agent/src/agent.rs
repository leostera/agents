use std::any::TypeId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use borg_llm::completion::{
    CompletionRequest, CompletionResponse, FinishReason, InputItem, ModelSelector, OutputContent,
    OutputItem, ResponseMode, TokenLimit, TopK, TopP,
};
use borg_llm::response::TypedResponse;
use borg_llm::runner::LlmRunner;
use borg_llm::tools::{ToolCall, TypedTool, TypedToolSet};
use borg_llm::{completion::Temperature, completion::ToolChoice};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::tools::{NoToolRunner, ToolCallEnvelope, ToolResultEnvelope, ToolRunner};

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
pub enum AgentEvent<C, T, R> {
    InputAccepted {
        item: InputItem,
    },
    TurnStarted {
        turn: u64,
    },
    ModelOutputItem {
        item: OutputItem<C, R>,
    },
    ToolCallRequested {
        call: ToolCallEnvelope<C>,
    },
    ToolExecutionStarted {
        call: ToolCallEnvelope<C>,
    },
    ToolExecutionCompleted {
        result: ToolResultEnvelope<T>,
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
pub struct TurnReport<C, T, R> {
    pub turn: u64,
    pub events: Vec<AgentEvent<C, T, R>>,
    pub outcome: TurnOutcome<R>,
}

pub struct AgentBuilder<C, T> {
    llm: Option<LlmRunner>,
    execution_profile: ExecutionProfile,
    tool_runner: Option<Arc<dyn ToolRunner<C, T>>>,
}

impl AgentBuilder<(), ()> {
    pub fn new() -> Self {
        Self {
            llm: None,
            execution_profile: ExecutionProfile::default(),
            tool_runner: Some(Arc::new(NoToolRunner)),
        }
    }
}

impl Default for AgentBuilder<(), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C, T> AgentBuilder<C, T>
where
    C: Clone + Send + Sync + 'static,
    T: Clone + Serialize + Send + Sync + 'static,
{
    pub fn with_llm_runner(mut self, llm: LlmRunner) -> Self {
        self.llm = Some(llm);
        self
    }

    pub fn with_execution_profile(mut self, execution_profile: ExecutionProfile) -> Self {
        self.execution_profile = execution_profile;
        self
    }

    pub fn with_tool_runner<C2, T2, R>(self, tool_runner: R) -> AgentBuilder<C2, T2>
    where
        C2: Clone + Send + Sync + 'static,
        T2: Clone + Serialize + Send + Sync + 'static,
        R: ToolRunner<C2, T2> + 'static,
    {
        AgentBuilder {
            llm: self.llm,
            execution_profile: self.execution_profile,
            tool_runner: Some(Arc::new(tool_runner)),
        }
    }

    pub fn build(self) -> AgentResult<Agent<C, T>> {
        let llm = self.llm.ok_or(AgentError::Internal {
            message: "AgentBuilder requires an llm runner".to_string(),
        })?;
        let tool_runner = self.tool_runner.ok_or(AgentError::Internal {
            message: "AgentBuilder requires a tool runner".to_string(),
        })?;

        Ok(Agent {
            llm,
            execution_profile: self.execution_profile,
            tool_runner,
            transcript: Mutex::new(Vec::new()),
            next_turn: AtomicU64::new(1),
        })
    }
}

pub struct Agent<C, T> {
    llm: LlmRunner,
    execution_profile: ExecutionProfile,
    tool_runner: Arc<dyn ToolRunner<C, T>>,
    transcript: Mutex<Vec<InputItem>>,
    next_turn: AtomicU64,
}

impl<C, T> Agent<C, T>
where
    C: TypedTool + Clone + Send + Sync + 'static,
    T: Clone + Serialize + Send + Sync + 'static,
{
    pub async fn send<M, R>(&self, input: AgentInput<M>) -> AgentResult<TurnReport<C, T, R>>
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
    ) -> AgentResult<TurnReport<C, T, R>>
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
    ) -> AgentResult<TurnReport<C, T, R>>
    where
        M: Into<InputItem> + Send,
        R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
    {
        let turn = self.next_turn.fetch_add(1, Ordering::SeqCst);
        let mut events = vec![AgentEvent::TurnStarted { turn }];

        let profile = profile_override.unwrap_or_else(|| self.execution_profile.clone());

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

                {
                    let mut transcript = self.transcript.lock().await;
                    transcript.push(item);
                }

                loop {
                    let request = {
                        let transcript = self.transcript.lock().await;
                        build_request::<C, R>(transcript.clone(), &profile)
                    };

                    let response = self.llm.chat::<C, R>(request).await?;

                    for item in response.output.iter().cloned() {
                        events.push(AgentEvent::ModelOutputItem { item });
                    }

                    let tool_calls = extract_tool_calls(&response);
                    if !tool_calls.is_empty() {
                        for call in tool_calls {
                            let envelope = ToolCallEnvelope {
                                call_id: call.id.clone(),
                                call: call.tool,
                            };
                            events.push(AgentEvent::ToolCallRequested {
                                call: envelope.clone(),
                            });
                            events.push(AgentEvent::ToolExecutionStarted {
                                call: envelope.clone(),
                            });

                            let result = self.tool_runner.run(envelope).await?;
                            let tool_result_item = encode_tool_result(&result)?;

                            {
                                let mut transcript = self.transcript.lock().await;
                                transcript.push(tool_result_item);
                            }

                            events.push(AgentEvent::ToolExecutionCompleted { result });
                        }

                        continue;
                    }

                    let reply = extract_reply(&response)?;
                    {
                        let mut transcript = self.transcript.lock().await;
                        transcript.push(assistant_item_for_reply(&reply)?);
                    }

                    events.push(AgentEvent::TurnCompleted {
                        turn,
                        finish_reason: response.finish_reason.clone(),
                    });
                    events.push(AgentEvent::Completed {
                        reply: reply.clone(),
                    });

                    return Ok(TurnReport {
                        turn,
                        events,
                        outcome: TurnOutcome::Completed { reply },
                    });
                }
            }
        }
    }

    pub async fn transcript(&self) -> Vec<InputItem> {
        self.transcript.lock().await.clone()
    }
}

impl Agent<(), ()> {
    pub fn builder() -> AgentBuilder<(), ()> {
        AgentBuilder::new()
    }
}

fn build_request<C, R>(input: Vec<InputItem>, profile: &ExecutionProfile) -> CompletionRequest<C, R>
where
    C: TypedTool,
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

    if TypeId::of::<C>() != TypeId::of::<()>() {
        request = request.with_tools(TypedToolSet::new());
    }

    if TypeId::of::<R>() != TypeId::of::<String>() {
        request = request.with_typed_response(TypedResponse::new("agent_response"));
    }

    request
}

fn extract_tool_calls<C, R>(response: &CompletionResponse<C, R>) -> Vec<ToolCall<C>>
where
    C: Clone,
{
    response
        .output
        .iter()
        .filter_map(|item| match item {
            OutputItem::ToolCall { call } => Some(call.clone()),
            OutputItem::Message { .. } | OutputItem::Reasoning { .. } => None,
        })
        .collect()
}

fn extract_reply<C, R>(response: &CompletionResponse<C, R>) -> AgentResult<R>
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

fn encode_tool_result<T>(result: &ToolResultEnvelope<T>) -> AgentResult<InputItem>
where
    T: Serialize,
{
    let content =
        serde_json::to_string(result).map_err(|error| AgentError::ToolResultEncoding {
            reason: error.to_string(),
        })?;
    Ok(InputItem::tool_result(result.call_id.clone(), content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use borg_llm::capability::Capability;
    use borg_llm::completion::{
        ProviderType, RawCompletionRequest, RawCompletionResponse, RawInputItem, RawOutputContent,
        RawOutputItem, Role,
    };
    use borg_llm::error::{Error as LlmError, LlmResult};
    use borg_llm::model::Model;
    use borg_llm::provider::LlmProvider;
    use borg_llm::tools::{RawToolCall, RawToolDefinition};
    use borg_llm::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};
    use serde::{Deserialize, Serialize};
    use std::collections::VecDeque;

    use crate::tools::{CallbackToolRunner, ToolExecutionResult};

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    struct EchoResponse {
        value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    enum TestTools {
        Ping { value: String },
    }

    impl TypedTool for TestTools {
        fn tool_definitions() -> Vec<RawToolDefinition> {
            vec![RawToolDefinition::function(
                "ping",
                Some("Ping tool"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"]
                }),
            )]
        }

        fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
            match name {
                "ping" => {
                    #[derive(Deserialize)]
                    struct PingArgs {
                        value: String,
                    }

                    let args: PingArgs = serde_json::from_value(arguments)
                        .map_err(|error| LlmError::parse("tool arguments", error))?;
                    Ok(TestTools::Ping { value: args.value })
                }
                other => Err(LlmError::InvalidResponse {
                    reason: format!("unknown tool {other}"),
                }),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct Pong {
        value: String,
    }

    struct FakeProvider {
        responses: Mutex<VecDeque<LlmResult<RawCompletionResponse>>>,
        requests: Mutex<Vec<RawCompletionRequest>>,
    }

    impl FakeProvider {
        fn with_responses(responses: Vec<LlmResult<RawCompletionResponse>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
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

    fn assistant_text_response(text: &str) -> RawCompletionResponse {
        RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![RawOutputItem::Message {
                role: Role::Assistant,
                content: vec![RawOutputContent::Text {
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
            output: vec![RawOutputItem::Message {
                role: Role::Assistant,
                content: vec![RawOutputContent::Json { value }],
            }],
            usage: borg_llm::completion::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
        }
    }

    fn tool_call_response() -> RawCompletionResponse {
        RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![RawOutputItem::ToolCall {
                call: RawToolCall {
                    id: "call_ping_1".to_string(),
                    name: "ping".to_string(),
                    arguments: serde_json::json!({ "value": "hello-tool" }),
                },
            }],
            usage: borg_llm::completion::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::ToolCalls,
        }
    }

    fn provider_error() -> LlmError {
        LlmError::Provider {
            provider: "openrouter".to_string(),
            status: 503,
            message: "temporarily unavailable".to_string(),
        }
    }

    fn ping_tool_runner() -> CallbackToolRunner<TestTools, Pong> {
        CallbackToolRunner::new(|call| async move {
            match call.call {
                TestTools::Ping { value } => Ok(ToolResultEnvelope {
                    call_id: call.call_id,
                    result: ToolExecutionResult::Ok {
                        data: Pong {
                            value: format!("pong:{value}"),
                        },
                    },
                }),
            }
        })
    }

    #[tokio::test]
    async fn builder_errors_without_llm_runner() {
        let result = Agent::builder().build();
        assert!(matches!(result, Err(AgentError::Internal { .. })));
    }

    #[tokio::test]
    async fn send_records_string_input_and_reply_in_transcript() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello back"),
                    )]))
                    .build(),
            )
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
    }

    #[tokio::test]
    async fn send_decodes_typed_response() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_json_response(serde_json::json!({ "value": "typed-ok" })),
                    )]))
                    .build(),
            )
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
    async fn send_executes_tool_calls_and_continues_to_final_reply() {
        let provider = Arc::new(FakeProvider::with_responses(vec![
            Ok(tool_call_response()),
            Ok(assistant_text_response("all done")),
        ]));

        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .with_tool_runner(ping_tool_runner())
            .build()
            .expect("agent");

        let report = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("ping please")))
            .await
            .expect("turn");

        assert!(matches!(
            report.outcome,
            TurnOutcome::Completed { ref reply } if reply == "all done"
        ));
        assert!(
            report
                .events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolCallRequested { .. }))
        );
        assert!(
            report
                .events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolExecutionCompleted { .. }))
        );

        let requests = provider.take_requests().await;
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            requests[1].input.last(),
            Some(RawInputItem::ToolResult { tool_use_id, content })
                if tool_use_id == "call_ping_1" && content.contains("pong:hello-tool")
        ));
    }

    #[tokio::test]
    async fn send_records_tool_errors_as_tool_results() {
        let provider = Arc::new(FakeProvider::with_responses(vec![
            Ok(tool_call_response()),
            Ok(assistant_text_response("tool error observed")),
        ]));

        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let failing_runner =
            CallbackToolRunner::new(|call: ToolCallEnvelope<TestTools>| async move {
                Ok(ToolResultEnvelope::<Pong> {
                    call_id: call.call_id,
                    result: ToolExecutionResult::Error {
                        message: "ping failed".to_string(),
                    },
                })
            });

        let agent = Agent::builder()
            .with_llm_runner(runner)
            .with_tool_runner(failing_runner)
            .build()
            .expect("agent");

        let report = agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("ping please")))
            .await
            .expect("turn");

        assert!(matches!(
            report.outcome,
            TurnOutcome::Completed { ref reply } if reply == "tool error observed"
        ));

        let requests = provider.take_requests().await;
        assert!(matches!(
            requests[1].input.last(),
            Some(RawInputItem::ToolResult { content, .. }) if content.contains("ping failed")
        ));
    }

    #[tokio::test]
    async fn cancel_does_not_call_llm() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![]))
                    .build(),
            )
            .build()
            .expect("agent");

        let report = agent
            .send::<InputItem, String>(AgentInput::Cancel)
            .await
            .expect("turn");

        assert!(matches!(report.outcome, TurnOutcome::Cancelled));
    }

    #[tokio::test]
    async fn send_reuses_prior_transcript_in_next_request() {
        let provider = Arc::new(FakeProvider::with_responses(vec![
            Ok(assistant_text_response("first reply")),
            Ok(assistant_text_response("second reply")),
        ]));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first turn");
        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect("second turn");

        let requests = provider.take_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].input.len(), 1);
        assert_eq!(requests[1].input.len(), 3);
    }

    #[tokio::test]
    async fn run_turn_returns_error_when_model_returns_no_matching_reply_type() {
        let response = RawCompletionResponse {
            provider: ProviderType::OpenAI,
            model: "test-model".to_string(),
            output: vec![RawOutputItem::Reasoning {
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
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(response)]))
                    .build(),
            )
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
        let provider = Arc::new(FakeProvider::with_responses(vec![Ok(
            assistant_text_response("hello back"),
        )]));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

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

        let requests = provider.take_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].token_limit, TokenLimit::Max(42));
    }

    #[tokio::test]
    async fn typed_send_sets_typed_response_format() {
        let provider = Arc::new(FakeProvider::with_responses(vec![Ok(
            assistant_json_response(serde_json::json!({ "value": "typed-ok" })),
        )]));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send::<_, EchoResponse>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        let requests = provider.take_requests().await;
        assert!(requests[0].response_format.is_some());
    }

    #[tokio::test]
    async fn string_send_does_not_set_typed_response_format() {
        let provider = Arc::new(FakeProvider::with_responses(vec![Ok(
            assistant_text_response("hello back"),
        )]));
        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send::<_, String>(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        let requests = provider.take_requests().await;
        assert!(requests[0].response_format.is_none());
    }

    #[tokio::test]
    async fn turn_events_include_model_output_and_completion() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello back"),
                    )]))
                    .build(),
            )
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
    }

    #[tokio::test]
    async fn send_propagates_llm_errors() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Err(provider_error())]))
                    .build(),
            )
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
