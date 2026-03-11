use std::any::TypeId;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

use borg_llm::completion::{
    CompletionRequest, CompletionResponse, InputItem, ModelSelector, OutputContent, OutputItem,
    ResponseMode, TokenLimit, TopK, TopP,
};
use borg_llm::response::TypedResponse;
use borg_llm::runner::LlmRunner;
use borg_llm::tools::{ToolCall, TypedTool, TypedToolSet};
use borg_llm::{completion::Temperature, completion::ToolChoice};
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

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
    ModelOutputItem { item: OutputItem<C, R> },
    ToolCallRequested { call: ToolCallEnvelope<C> },
    ToolExecutionCompleted { result: ToolResultEnvelope<T> },
    Completed { reply: R },
    Cancelled,
}

pub struct AgentBuilder<M, C, T, R> {
    llm: Option<LlmRunner>,
    execution_profile: ExecutionProfile,
    tool_runner: Option<Arc<dyn ToolRunner<C, T>>>,
    _message: PhantomData<M>,
    _response: PhantomData<R>,
}

impl AgentBuilder<InputItem, (), (), String> {
    pub fn new() -> Self {
        Self {
            llm: None,
            execution_profile: ExecutionProfile::default(),
            tool_runner: Some(Arc::new(NoToolRunner)),
            _message: PhantomData,
            _response: PhantomData,
        }
    }
}

impl Default for AgentBuilder<InputItem, (), (), String> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, C, T, R> AgentBuilder<M, C, T, R>
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

    pub fn with_message_type<M2>(self) -> AgentBuilder<M2, C, T, R> {
        AgentBuilder {
            llm: self.llm,
            execution_profile: self.execution_profile,
            tool_runner: self.tool_runner,
            _message: PhantomData,
            _response: PhantomData,
        }
    }

    pub fn with_response_type<R2>(self) -> AgentBuilder<M, C, T, R2> {
        AgentBuilder {
            llm: self.llm,
            execution_profile: self.execution_profile,
            tool_runner: self.tool_runner,
            _message: PhantomData,
            _response: PhantomData,
        }
    }

    pub fn with_tool_runner<C2, T2, Runner>(self, tool_runner: Runner) -> AgentBuilder<M, C2, T2, R>
    where
        C2: Clone + Send + Sync + 'static,
        T2: Clone + Serialize + Send + Sync + 'static,
        Runner: ToolRunner<C2, T2> + 'static,
    {
        AgentBuilder {
            llm: self.llm,
            execution_profile: self.execution_profile,
            tool_runner: Some(Arc::new(tool_runner)),
            _message: PhantomData,
            _response: PhantomData,
        }
    }
}

impl<M, C, T, R> AgentBuilder<M, C, T, R>
where
    M: Into<InputItem> + Send + 'static,
    C: TypedTool + Clone + Send + Sync + 'static,
    T: Clone + Serialize + Send + Sync + 'static,
    R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    pub fn build(self) -> AgentResult<Agent<M, C, T, R>> {
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
            transcript: Vec::new(),
            next_turn: 1,
            active_turn: None,
            _message: PhantomData,
            _response: PhantomData,
        })
    }
}

pub struct Agent<M, C, T, R> {
    llm: LlmRunner,
    execution_profile: ExecutionProfile,
    tool_runner: Arc<dyn ToolRunner<C, T>>,
    transcript: Vec<InputItem>,
    next_turn: u64,
    active_turn: Option<ActiveTurn<C, T, R>>,
    _message: PhantomData<M>,
    _response: PhantomData<R>,
}

struct ActiveTurn<C, T, R> {
    turn: u64,
    profile: ExecutionProfile,
    state: TurnState<C, T, R>,
}

enum TurnState<C, T, R> {
    CancelPending,
    NeedLlm,
    ExecuteTool {
        current: ToolCallEnvelope<C>,
        remaining: VecDeque<ToolCallEnvelope<C>>,
    },
    EmitQueue {
        queue: VecDeque<AgentEvent<C, T, R>>,
        next: Box<TurnState<C, T, R>>,
    },
    Done,
}

impl<M, C, T, R> Agent<M, C, T, R>
where
    M: Into<InputItem> + Send + 'static,
    C: TypedTool + Clone + Send + Sync + 'static,
    T: Clone + Serialize + Send + Sync + 'static,
    R: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    pub async fn send(&mut self, input: AgentInput<M>) -> AgentResult<()> {
        let profile = self.execution_profile.clone();
        self.send_with_profile(input, profile).await
    }

    pub async fn send_with_profile(
        &mut self,
        input: AgentInput<M>,
        profile: ExecutionProfile,
    ) -> AgentResult<()> {
        if self.active_turn.is_some() {
            return Err(AgentError::InvalidInput {
                reason: "cannot send a new input while a turn is in progress".to_string(),
            });
        }

        let turn = self.next_turn;
        self.next_turn += 1;

        let state = match input {
            AgentInput::Cancel => TurnState::CancelPending,
            AgentInput::Message(message) => {
                self.transcript.push(message.into());
                TurnState::NeedLlm
            }
        };

        self.active_turn = Some(ActiveTurn {
            turn,
            profile,
            state,
        });

        Ok(())
    }

    pub async fn next(&mut self) -> AgentResult<Option<AgentEvent<C, T, R>>> {
        loop {
            let Some(mut active_turn) = self.active_turn.take() else {
                return Ok(None);
            };

            let state = std::mem::replace(&mut active_turn.state, TurnState::Done);

            match state {
                TurnState::Done => {
                    return Ok(None);
                }
                TurnState::CancelPending => {
                    return Ok(Some(AgentEvent::Cancelled));
                }
                TurnState::NeedLlm => {
                    let request =
                        build_request::<C, R>(self.transcript.clone(), &active_turn.profile);
                    let response = self.llm.chat::<C, R>(request).await?;
                    active_turn.state = self.turn_state_from_response(response).await?;
                    self.active_turn = Some(active_turn);
                }
                TurnState::ExecuteTool {
                    current,
                    mut remaining,
                } => {
                    let result = self.tool_runner.run(current).await?;
                    let tool_result_item = encode_tool_result(&result)?;
                    self.transcript.push(tool_result_item);

                    active_turn.state = if let Some(next_call) = remaining.pop_front() {
                        TurnState::EmitQueue {
                            queue: VecDeque::from([AgentEvent::ToolCallRequested {
                                call: next_call.clone(),
                            }]),
                            next: Box::new(TurnState::ExecuteTool {
                                current: next_call,
                                remaining,
                            }),
                        }
                    } else {
                        TurnState::NeedLlm
                    };

                    self.active_turn = Some(active_turn);
                    return Ok(Some(AgentEvent::ToolExecutionCompleted { result }));
                }
                TurnState::EmitQueue { mut queue, next } => {
                    if let Some(event) = queue.pop_front() {
                        active_turn.state = if queue.is_empty() {
                            *next
                        } else {
                            TurnState::EmitQueue { queue, next }
                        };
                        self.active_turn = Some(active_turn);
                        return Ok(Some(event));
                    }

                    active_turn.state = *next;
                    self.active_turn = Some(active_turn);
                }
            }
        }
    }

    pub fn transcript(&self) -> &[InputItem] {
        &self.transcript
    }

    pub fn active_turn(&self) -> Option<u64> {
        self.active_turn.as_ref().map(|turn| turn.turn)
    }

    async fn turn_state_from_response(
        &mut self,
        response: CompletionResponse<C, R>,
    ) -> AgentResult<TurnState<C, T, R>> {
        let non_tool_items = response
            .output
            .iter()
            .filter(|item| !matches!(item, OutputItem::ToolCall { .. }))
            .cloned()
            .map(|item| AgentEvent::ModelOutputItem { item })
            .collect::<Vec<_>>();

        let tool_calls = extract_tool_calls(&response)
            .into_iter()
            .map(|call| ToolCallEnvelope {
                call_id: call.id,
                call: call.tool,
            })
            .collect::<VecDeque<_>>();

        if let Some(current) = tool_calls.front().cloned() {
            let mut queue = VecDeque::from(non_tool_items);
            queue.push_back(AgentEvent::ToolCallRequested {
                call: current.clone(),
            });

            let mut remaining = tool_calls;
            let _ = remaining.pop_front();

            return Ok(TurnState::EmitQueue {
                queue,
                next: Box::new(TurnState::ExecuteTool { current, remaining }),
            });
        }

        let reply = extract_reply(&response)?;
        self.transcript.push(assistant_item_for_reply(&reply)?);

        let mut queue = VecDeque::from(non_tool_items);
        queue.push_back(AgentEvent::Completed { reply });

        Ok(TurnState::EmitQueue {
            queue,
            next: Box::new(TurnState::Done),
        })
    }
}

impl Agent<InputItem, (), (), String> {
    pub fn builder() -> AgentBuilder<InputItem, (), (), String> {
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
        FinishReason, ProviderType, RawCompletionRequest, RawCompletionResponse, RawInputItem,
        RawOutputContent, RawOutputItem, Role,
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
        responses: std::sync::Mutex<VecDeque<LlmResult<RawCompletionResponse>>>,
        requests: std::sync::Mutex<Vec<RawCompletionRequest>>,
    }

    impl FakeProvider {
        fn with_responses(responses: Vec<LlmResult<RawCompletionResponse>>) -> Self {
            Self {
                responses: std::sync::Mutex::new(VecDeque::from(responses)),
                requests: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn take_requests(&self) -> Vec<RawCompletionRequest> {
            self.requests.lock().expect("requests").clone()
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
            self.requests.lock().expect("requests").push(req);
            self.responses
                .lock()
                .expect("responses")
                .pop_front()
                .unwrap_or_else(|| {
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
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello back"),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "hello back"
        ));
        assert!(agent.next().await.expect("done").is_none());
        assert_eq!(agent.transcript().len(), 2);
    }

    #[tokio::test]
    async fn send_decodes_typed_response() {
        let mut agent = Agent::builder()
            .with_response_type::<EchoResponse>()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_json_response(serde_json::json!({ "value": "typed-ok" })),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply: EchoResponse { value } }) if value == "typed-ok"
        ));
        assert!(agent.next().await.expect("done").is_none());
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
        let mut agent = Agent::builder()
            .with_tool_runner(ping_tool_runner())
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("ping please")))
            .await
            .expect("turn");

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ToolCallRequested { call }) if call.call_id == "call_ping_1"
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ToolExecutionCompleted { result }) if result.call_id == "call_ping_1"
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "all done"
        ));
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
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

        let mut agent = Agent::builder()
            .with_tool_runner(failing_runner)
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("ping please")))
            .await
            .expect("turn");
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ToolCallRequested { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ToolExecutionCompleted { result })
                if matches!(result.result, ToolExecutionResult::Error { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "tool error observed"
        ));
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
        assert!(matches!(
            requests[1].input.last(),
            Some(RawInputItem::ToolResult { content, .. }) if content.contains("ping failed")
        ));
    }

    #[tokio::test]
    async fn cancel_does_not_call_llm() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent.send(AgentInput::Cancel).await.expect("turn");
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Cancelled)
        ));
        assert!(agent.next().await.expect("done").is_none());
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
        let mut agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first turn");
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("done").is_none());

        agent
            .send(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect("second turn");
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].input.len(), 1);
        assert_eq!(requests[1].input.len(), 3);
    }

    #[tokio::test]
    async fn send_errors_when_turn_already_in_progress() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello"),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first send");

        let error = agent
            .send(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect_err("should fail");

        assert!(matches!(error, AgentError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn next_errors_when_model_returns_no_matching_reply_type() {
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

        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(response)]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        let error = agent.next().await.expect_err("should fail");

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
        let mut agent = Agent::builder()
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
            .send_with_profile(AgentInput::Message(InputItem::user_text("hello")), profile)
            .await
            .expect("turn");
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
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
        let mut agent = Agent::builder()
            .with_response_type::<EchoResponse>()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
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
        let mut agent = Agent::builder()
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("next").is_some());
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
        assert!(requests[0].response_format.is_none());
    }

    #[tokio::test]
    async fn next_emits_model_output_before_completion() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello back"),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { .. })
        ));
        assert!(agent.next().await.expect("done").is_none());
    }

    #[tokio::test]
    async fn next_propagates_llm_errors() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Err(provider_error())]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");

        let error = agent.next().await.expect_err("should fail");

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
