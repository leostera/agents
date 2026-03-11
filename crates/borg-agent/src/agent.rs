use std::any::TypeId;
use std::collections::HashSet;
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
use tokio::sync::mpsc;

use crate::context::{ContextChunk, ContextManager, ContextStrategy, ContextWindow};
use crate::error::{AgentError, AgentResult};
use crate::tools::{
    NoToolRunner, ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope, ToolRunner,
};

#[derive(Debug, Clone)]
pub enum AgentInput<M> {
    Message(M),
    Steer(M),
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

pub type AgentRunInput<M> = mpsc::Sender<AgentInput<M>>;
pub type AgentRunOutput<C, T, R> = mpsc::Receiver<AgentResult<AgentEvent<C, T, R>>>;

const DEFAULT_RUN_CHANNEL_CAPACITY: usize = 64;

pub struct AgentBuilder<M, C, T, R> {
    llm: Option<Arc<LlmRunner>>,
    context_manager: ContextManager,
    execution_profile: ExecutionProfile,
    run_channel_capacity: usize,
    tool_runner: Option<Arc<dyn ToolRunner<C, T>>>,
    _message: PhantomData<M>,
    _response: PhantomData<R>,
}

impl AgentBuilder<InputItem, (), (), String> {
    pub fn new() -> Self {
        Self {
            llm: None,
            context_manager: ContextManager::new(),
            execution_profile: ExecutionProfile::default(),
            run_channel_capacity: DEFAULT_RUN_CHANNEL_CAPACITY,
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
        self.llm = Some(Arc::new(llm));
        self
    }

    pub fn with_execution_profile(mut self, execution_profile: ExecutionProfile) -> Self {
        self.execution_profile = execution_profile;
        self
    }

    pub fn with_context_manager(mut self, context_manager: ContextManager) -> Self {
        self.context_manager = context_manager;
        self
    }

    pub fn with_run_channel_capacity(mut self, capacity: usize) -> Self {
        self.run_channel_capacity = capacity.max(1);
        self
    }

    pub fn with_message_type<M2>(self) -> AgentBuilder<M2, C, T, R> {
        AgentBuilder {
            llm: self.llm,
            context_manager: self.context_manager,
            execution_profile: self.execution_profile,
            run_channel_capacity: self.run_channel_capacity,
            tool_runner: self.tool_runner,
            _message: PhantomData,
            _response: PhantomData,
        }
    }

    pub fn with_response_type<R2>(self) -> AgentBuilder<M, C, T, R2> {
        AgentBuilder {
            llm: self.llm,
            context_manager: self.context_manager,
            execution_profile: self.execution_profile,
            run_channel_capacity: self.run_channel_capacity,
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
            context_manager: self.context_manager,
            execution_profile: self.execution_profile,
            run_channel_capacity: self.run_channel_capacity,
            tool_runner: Some(Arc::new(tool_runner)),
            _message: PhantomData,
            _response: PhantomData,
        }
    }
}

impl<M, C, T, R> AgentBuilder<M, C, T, R>
where
    M: Into<InputItem> + Send + Sync + 'static,
    C: TypedTool + Clone + Serialize + Send + Sync + 'static,
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
        let context_manager = self.context_manager;
        context_manager.attach_llm_runner(llm.clone());

        Ok(Agent {
            llm,
            context_manager: Arc::new(context_manager),
            execution_profile: self.execution_profile,
            run_channel_capacity: self.run_channel_capacity,
            tool_runner,
            next_turn: 1,
            active_turn: None,
            queued_turns: VecDeque::new(),
            _message: PhantomData,
            _response: PhantomData,
        })
    }
}

pub struct Agent<M, C, T, R> {
    llm: Arc<LlmRunner>,
    context_manager: Arc<ContextManager>,
    execution_profile: ExecutionProfile,
    run_channel_capacity: usize,
    tool_runner: Arc<dyn ToolRunner<C, T>>,
    next_turn: u64,
    active_turn: Option<ActiveTurn<C, T, R>>,
    queued_turns: VecDeque<QueuedTurn>,
    _message: PhantomData<M>,
    _response: PhantomData<R>,
}

struct ActiveTurn<C, T, R> {
    turn: u64,
    profile: ExecutionProfile,
    state: TurnState<C, T, R>,
}

struct QueuedTurn {
    turn: u64,
    profile: ExecutionProfile,
    item: InputItem,
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
    M: Into<InputItem> + Send + Sync + 'static,
    C: TypedTool + Clone + Serialize + Send + Sync + 'static,
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
        match input {
            AgentInput::Cancel => {
                if let Some(active_turn) = self.active_turn.as_mut() {
                    for result in abandoned_tool_results(&active_turn.state, "cancelled") {
                        self.context_manager
                            .push(tool_result_to_chunk(&result, ContextStrategy::Compactable)?)
                            .await?;
                    }
                    active_turn.state = TurnState::CancelPending;
                }
                Ok(())
            }
            AgentInput::Message(message) => {
                let item = message.into();
                if self.active_turn.is_some() {
                    self.queue_turn(item, profile);
                } else {
                    self.start_turn(item, profile).await?;
                }
                Ok(())
            }
            AgentInput::Steer(message) => {
                let item = message.into();
                if self.active_turn.is_some() {
                    if let Some(active_turn) = self.active_turn.as_mut() {
                        for result in
                            abandoned_tool_results(&active_turn.state, "interrupted by steering")
                        {
                            self.context_manager
                                .push(tool_result_to_chunk(&result, ContextStrategy::Compactable)?)
                                .await?;
                        }
                        active_turn.state = TurnState::NeedLlm;
                    }
                    self.context_manager
                        .push(input_item_to_chunk(item, ContextStrategy::Compactable)?)
                        .await?;
                } else {
                    self.start_turn(item, profile).await?;
                }
                Ok(())
            }
        }
    }

    pub async fn next(&mut self) -> AgentResult<Option<AgentEvent<C, T, R>>> {
        loop {
            if self.active_turn.is_none() {
                if let Some(queued_turn) = self.queued_turns.pop_front() {
                    self.context_manager
                        .push(input_item_to_chunk(
                            queued_turn.item,
                            ContextStrategy::Compactable,
                        )?)
                        .await?;
                    self.active_turn = Some(ActiveTurn {
                        turn: queued_turn.turn,
                        profile: queued_turn.profile,
                        state: TurnState::NeedLlm,
                    });
                } else {
                    return Ok(None);
                }
            }

            let Some(mut active_turn) = self.active_turn.take() else {
                return Ok(None);
            };

            let state = std::mem::replace(&mut active_turn.state, TurnState::Done);

            match state {
                TurnState::Done => {
                    continue;
                }
                TurnState::CancelPending => {
                    return Ok(Some(AgentEvent::Cancelled));
                }
                TurnState::NeedLlm => {
                    let request = self.build_request(active_turn.profile.clone()).await?;
                    let response = self.llm.chat::<C, R>(request).await?;
                    active_turn.state = self.turn_state_from_response(response).await?;
                    self.active_turn = Some(active_turn);
                }
                TurnState::ExecuteTool {
                    current,
                    mut remaining,
                } => {
                    let result = self.tool_runner.run(current).await?;
                    self.context_manager
                        .push(tool_result_to_chunk(&result, ContextStrategy::Compactable)?)
                        .await?;

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

    pub async fn run(mut self) -> AgentResult<(AgentRunInput<M>, AgentRunOutput<C, T, R>)> {
        let (input_tx, mut input_rx) = mpsc::channel(self.run_channel_capacity);
        let (event_tx, event_rx) = mpsc::channel(self.run_channel_capacity);

        tokio::spawn(async move {
            let mut input_closed = false;

            loop {
                while let Ok(input) = input_rx.try_recv() {
                    if let Err(error) = self.send(input).await
                        && event_tx.send(Err(error)).await.is_err()
                    {
                        return;
                    }
                }

                match self.next().await {
                    Ok(Some(event)) => {
                        if event_tx.send(Ok(event)).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        if input_closed {
                            return;
                        }

                        match input_rx.recv().await {
                            Some(input) => {
                                if let Err(error) = self.send(input).await
                                    && event_tx.send(Err(error)).await.is_err()
                                {
                                    return;
                                }
                            }
                            None => {
                                input_closed = true;
                            }
                        }
                    }
                    Err(error) => {
                        if event_tx.send(Err(error)).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok((input_tx, event_rx))
    }

    pub async fn transcript(&self) -> AgentResult<Vec<InputItem>> {
        ContextWindow::new(self.context_manager.history().await?).to_input_items()
    }

    pub fn active_turn(&self) -> Option<u64> {
        self.active_turn.as_ref().map(|turn| turn.turn)
    }

    pub fn queued_turn_count(&self) -> usize {
        self.queued_turns.len()
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
                name: call.name,
                call: call.tool,
            })
            .collect::<VecDeque<_>>();

        for call in &tool_calls {
            self.context_manager
                .push(tool_call_to_chunk(call, ContextStrategy::Compactable)?)
                .await?;
        }

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

        let reply = self.extract_reply(&response)?;
        self.context_manager
            .push(reply_to_chunk(&reply, ContextStrategy::Compactable)?)
            .await?;

        let mut queue = VecDeque::from(non_tool_items);
        queue.push_back(AgentEvent::Completed { reply });

        Ok(TurnState::EmitQueue {
            queue,
            next: Box::new(TurnState::Done),
        })
    }

    async fn start_turn(&mut self, item: InputItem, profile: ExecutionProfile) -> AgentResult<()> {
        let turn = self.next_turn;
        self.next_turn += 1;
        self.context_manager
            .push(input_item_to_chunk(item, ContextStrategy::Compactable)?)
            .await?;
        self.active_turn = Some(ActiveTurn {
            turn,
            profile,
            state: TurnState::NeedLlm,
        });
        Ok(())
    }

    fn queue_turn(&mut self, item: InputItem, profile: ExecutionProfile) {
        let turn = self.next_turn;
        self.next_turn += 1;
        self.queued_turns.push_back(QueuedTurn {
            turn,
            profile,
            item,
        });
    }

    async fn build_request(
        &self,
        profile: ExecutionProfile,
    ) -> AgentResult<CompletionRequest<C, R>> {
        let window = self.context_manager.window().await?;
        let mut request = CompletionRequest::new(window.to_input_items()?, profile.model_selector)
            .with_token_limit(profile.token_limit)
            .with_tool_choice(profile.tool_choice)
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

        Ok(request)
    }

    fn extract_reply(&self, response: &CompletionResponse<C, R>) -> AgentResult<R> {
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
}

impl Agent<InputItem, (), (), String> {
    pub fn builder() -> AgentBuilder<InputItem, (), (), String> {
        AgentBuilder::new()
    }
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

fn input_item_to_chunk(item: InputItem, strategy: ContextStrategy) -> AgentResult<ContextChunk> {
    ContextChunk::from_input_item(strategy, item).unwrap_or_else(|| {
        Err(AgentError::InvalidInput {
            reason: "unable to convert input item into context chunk".to_string(),
        })
    })
}

fn reply_to_chunk<R>(reply: &R, strategy: ContextStrategy) -> AgentResult<ContextChunk>
where
    R: Serialize + 'static,
{
    input_item_to_chunk(assistant_item_for_reply(reply)?, strategy)
}

fn tool_call_to_chunk<C>(
    call: &ToolCallEnvelope<C>,
    strategy: ContextStrategy,
) -> AgentResult<ContextChunk>
where
    C: Serialize,
{
    let args = serde_json::to_value(&call.call).map_err(|error| AgentError::Internal {
        message: error.to_string(),
    })?;

    Ok(ContextChunk::ToolCall {
        strategy,
        id: call.call_id.clone(),
        name: call.name.clone(),
        args,
    })
}

fn tool_result_to_chunk<T>(
    result: &ToolResultEnvelope<T>,
    strategy: ContextStrategy,
) -> AgentResult<ContextChunk>
where
    T: Serialize,
{
    let value = serde_json::to_value(result).map_err(|error| AgentError::ToolResultEncoding {
        reason: error.to_string(),
    })?;

    Ok(ContextChunk::ToolResult {
        strategy,
        id: result.call_id.clone(),
        result: value,
    })
}

fn abandoned_tool_results<C, T, R>(
    state: &TurnState<C, T, R>,
    reason: &str,
) -> Vec<ToolResultEnvelope<T>> {
    let mut call_ids = Vec::new();
    let mut seen = HashSet::new();
    collect_pending_tool_call_ids(state, &mut call_ids, &mut seen);
    call_ids
        .into_iter()
        .map(|call_id| ToolResultEnvelope {
            call_id,
            result: ToolExecutionResult::Error {
                message: reason.to_string(),
            },
        })
        .collect()
}

fn collect_pending_tool_call_ids<C, T, R>(
    state: &TurnState<C, T, R>,
    call_ids: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    match state {
        TurnState::ExecuteTool { current, remaining } => {
            if seen.insert(current.call_id.clone()) {
                call_ids.push(current.call_id.clone());
            }
            for call in remaining {
                if seen.insert(call.call_id.clone()) {
                    call_ids.push(call.call_id.clone());
                }
            }
        }
        TurnState::EmitQueue { queue, next } => {
            for event in queue {
                if let AgentEvent::ToolCallRequested { call } = event
                    && seen.insert(call.call_id.clone())
                {
                    call_ids.push(call.call_id.clone());
                }
            }
            collect_pending_tool_call_ids(next, call_ids, seen);
        }
        TurnState::CancelPending | TurnState::NeedLlm | TurnState::Done => {}
    }
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

    use crate::context::{ContextManager, StaticContextProvider};
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
        assert_eq!(agent.transcript().await.expect("transcript").len(), 2);
    }

    #[tokio::test]
    async fn static_context_provider_is_included_in_llm_request() {
        let provider = Arc::new(FakeProvider::with_responses(vec![Ok(
            assistant_text_response("hello back"),
        )]));

        let runner = LlmRunner::builder()
            .add_provider(ArcBackedFakeProvider(provider.clone()))
            .build();

        let mut agent = Agent::builder()
            .with_context_manager(
                ContextManager::builder()
                    .add_provider(StaticContextProvider::system_text("You are a test agent."))
                    .build(),
            )
            .with_llm_runner(runner)
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("turn");
        let _ = agent.next().await.expect("model output");
        let _ = agent.next().await.expect("completed");

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        assert!(matches!(
            requests[0].input.first(),
            Some(RawInputItem::Message { role: Role::System, content })
                if matches!(content.first(), Some(borg_llm::completion::RawInputContent::Text { text }) if text == "You are a test agent.")
        ));
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
    async fn run_streams_text_turn_events() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("hello from run"),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        let (tx, mut rx) = agent.run().await.expect("run");
        tx.send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("send input");
        drop(tx);

        assert!(matches!(
            rx.recv().await.expect("model item"),
            Ok(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            rx.recv().await.expect("completed"),
            Ok(AgentEvent::Completed { reply }) if reply == "hello from run"
        ));
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn run_streams_tool_sequence() {
        let agent = Agent::builder()
            .with_tool_runner(ping_tool_runner())
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![
                        Ok(tool_call_response()),
                        Ok(assistant_text_response("done after tool")),
                    ]))
                    .build(),
            )
            .build()
            .expect("agent");

        let (tx, mut rx) = agent.run().await.expect("run");
        tx.send(AgentInput::Message(InputItem::user_text("ping please")))
            .await
            .expect("send input");
        drop(tx);

        assert!(matches!(
            rx.recv().await.expect("tool call"),
            Ok(AgentEvent::ToolCallRequested { call }) if call.call_id == "call_ping_1"
        ));
        assert!(matches!(
            rx.recv().await.expect("tool result"),
            Ok(AgentEvent::ToolExecutionCompleted { result }) if result.call_id == "call_ping_1"
        ));
        assert!(matches!(
            rx.recv().await.expect("model item"),
            Ok(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            rx.recv().await.expect("completed"),
            Ok(AgentEvent::Completed { reply }) if reply == "done after tool"
        ));
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn run_processes_multiple_inputs_in_order() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![
                        Ok(assistant_text_response("first")),
                        Ok(assistant_text_response("second")),
                    ]))
                    .build(),
            )
            .build()
            .expect("agent");

        let (tx, mut rx) = agent.run().await.expect("run");
        tx.send(AgentInput::Message(InputItem::user_text("one")))
            .await
            .expect("first input");
        tx.send(AgentInput::Message(InputItem::user_text("two")))
            .await
            .expect("second input");
        drop(tx);

        assert!(matches!(
            rx.recv().await.expect("first model item"),
            Ok(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            rx.recv().await.expect("first completed"),
            Ok(AgentEvent::Completed { reply }) if reply == "first"
        ));
        assert!(matches!(
            rx.recv().await.expect("second model item"),
            Ok(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            rx.recv().await.expect("second completed"),
            Ok(AgentEvent::Completed { reply }) if reply == "second"
        ));
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn run_surfaces_agent_errors() {
        let agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Err(provider_error())]))
                    .build(),
            )
            .build()
            .expect("agent");

        let (tx, mut rx) = agent.run().await.expect("run");
        tx.send(AgentInput::Message(InputItem::user_text("hello")))
            .await
            .expect("input");
        drop(tx);

        assert!(matches!(
            rx.recv().await.expect("error event"),
            Err(AgentError::Llm(source))
                if matches!(source, LlmError::Provider { status: 503, .. })
        ));
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn cancel_while_idle_is_a_no_op() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent.send(AgentInput::Cancel).await.expect("turn");
        assert!(agent.next().await.expect("idle").is_none());
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
    async fn send_queues_messages_while_turn_is_in_progress() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![
                        Ok(assistant_text_response("first")),
                        Ok(assistant_text_response("second")),
                    ]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Message(InputItem::user_text("first")))
            .await
            .expect("first send");
        assert_eq!(agent.queued_turn_count(), 0);

        agent
            .send(AgentInput::Message(InputItem::user_text("second")))
            .await
            .expect("second send");
        assert_eq!(agent.queued_turn_count(), 1);

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "first"
        ));
        match agent.next().await.expect("next") {
            Some(AgentEvent::ModelOutputItem { .. }) => {
                assert!(matches!(
                    agent.next().await.expect("next"),
                    Some(AgentEvent::Completed { reply }) if reply == "second"
                ));
            }
            Some(AgentEvent::Completed { reply }) => {
                assert_eq!(reply, "second");
            }
            other => panic!("expected queued turn event, got {other:?}"),
        }
        assert!(agent.next().await.expect("done").is_none());
    }

    #[tokio::test]
    async fn steer_while_idle_behaves_like_message() {
        let mut agent = Agent::builder()
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(
                        assistant_text_response("steered"),
                    )]))
                    .build(),
            )
            .build()
            .expect("agent");

        agent
            .send(AgentInput::Steer(InputItem::user_text("hello")))
            .await
            .expect("steer");

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "steered"
        ));
        assert!(agent.next().await.expect("done").is_none());
    }

    #[tokio::test]
    async fn steer_during_pending_tool_plan_clears_remaining_tool_calls() {
        let provider = Arc::new(FakeProvider::with_responses(vec![
            Ok(tool_call_response()),
            Ok(assistant_text_response("steered reply")),
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
            .expect("first");

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ToolCallRequested { .. })
        ));

        agent
            .send(AgentInput::Steer(InputItem::user_text(
                "Do not call any tool. Reply with 'steered reply'.",
            )))
            .await
            .expect("steer");

        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::ModelOutputItem { .. })
        ));
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Completed { reply }) if reply == "steered reply"
        ));
        assert!(agent.next().await.expect("done").is_none());

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].input.iter().any(|item| {
            matches!(
                item,
                RawInputItem::ToolResult {
                    tool_use_id,
                    content,
                } if tool_use_id == "call_ping_1" && content.contains("interrupted by steering")
            )
        }));
        assert!(matches!(
            requests[1].input.last(),
            Some(RawInputItem::Message {
                role: Role::User,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn cancel_during_active_turn_finishes_immediately() {
        let mut agent = Agent::builder()
            .with_tool_runner(ping_tool_runner())
            .with_llm_runner(
                LlmRunner::builder()
                    .add_provider(FakeProvider::with_responses(vec![Ok(tool_call_response())]))
                    .build(),
            )
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

        agent.send(AgentInput::Cancel).await.expect("cancel");
        assert!(matches!(
            agent.next().await.expect("next"),
            Some(AgentEvent::Cancelled)
        ));
        assert!(agent.next().await.expect("done").is_none());

        let transcript = agent.transcript().await.expect("transcript");
        assert!(transcript.iter().any(|item| {
            matches!(
                item,
                InputItem::ToolResult {
                    tool_use_id,
                    content,
                } if tool_use_id == "call_ping_1" && content.contains("cancelled")
            )
        }));
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
