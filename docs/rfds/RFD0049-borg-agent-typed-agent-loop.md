# RFD0049 - Typed Agent Loop for `borg-agent`

- Feature Name: `typed-agent-loop`
- Start Date: 2026-03-11
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary

Add a new `borg-agent` crate that provides a type-safe agentic loop over:

- `LlmRunner`
- `ContextManager`
- `ToolRunner`

The public API should let callers start a session-oriented `Agent`, send typed inputs into it over a stream, and receive a streamed sequence of typed agent events and replies. Internally, the agent owns the orchestration loop:

1. decode the incoming application message into agent input items
2. update and query its own context via `ContextManager`
3. call `LlmRunner`
4. stream model outputs as agent events
5. decode and execute typed tool calls through `ToolRunner`
6. validate typed tool results before feeding them back to the model
7. continue until the turn completes or errors

`borg-agent` should be typed at the application boundary and item-based internally, matching the new `borg-llm` request/response model.

Internally, the agent should be implemented as a turn-based machine. The long-lived session APIs should be thin wrappers over a single-turn primitive.

## Motivation

`borg-llm` now gives us:

- provider-neutral input/output items
- typed tool-call decoding
- typed structured responses
- buffered and streamed execution

What it intentionally does not do is run an agent loop. Today, every caller would need to rebuild the same control flow:

- maintain conversation state
- decide what context to include
- stream model output
- execute tools
- turn tool results back into input items
- decide whether another model turn is needed

That logic is central enough that it deserves its own crate, but it should not repeat the design mistakes of the old `crates.old/borg-agent` implementation:

- provider-facing messages as the core state model
- context assembled as a one-shot prompt window rather than a first-class state manager
- weakly enforced tool/result boundaries

`borg-agent` should start small and strongly typed:

- one explicit orchestration loop
- one first-class context subsystem
- one first-class tool execution subsystem
- one stable event protocol

## Design Constraints

The loop should be:

- easy to read as ordinary Rust control flow
- typed at the application boundary
- item-based internally, matching `borg-llm`
- explicit about context ownership
- explicit about tool-call and tool-result boundaries

The loop should not:

- know provider wire formats
- treat context as just a prompt-building helper
- collapse tool execution into ad hoc closures in the main loop
- couple v1 to approvals, sandboxing, or realtime transport concerns

## Lessons from the old `crates.old/borg-agent`

The old crate has useful instincts:

- typed tool-call and tool-result generics
- a `ContextManager`
- a `Toolchain`

But it should not be the base:

- it centers provider-facing message shapes
- it serializes/deserializes at the wrong boundary
- it treats context as a prompt window builder rather than a conversation state owner

## Goals

- provide a small, explicit, type-safe agent loop on top of `borg-llm`
- keep provider details out of `borg-agent`
- make context ownership explicit
- make tool-call and tool-result typing explicit
- provide a streamed event API suitable for UI, orchestration, and broadcasting
- support buffered and streamed LLM turns through the same agent interface
- make the loop unit-testable with fake `LlmRunner`, `ContextManager`, and `ToolRunner`

## Non-Goals

- approval systems
- sandboxing policy
- realtime audio sessions
- background task delegation
- persistence format or database schema
- provider-specific retries/fallback policy

Those can be layered around `borg-agent` later.

## Guide-Level Explanation

### Shape of the API

```rust
let agent = Agent::builder()
    .with_llm_runner(llm_runner)
    .with_context_manager(context_manager)
    .with_tool_runner(tool_runner)
    .with_execution_profile(ExecutionProfile::default())
    .build();

let (tx, mut rx) = agent
    .run::<AppMessage, FinalReply, AppToolCall, AppToolResult>()
    .await?;

tx.send(AgentInput::Message(message)).await?;

while let Some(event) = rx.recv().await {
    match event? {
        AgentEvent::InputAccepted { .. } => {}
        AgentEvent::ModelOutputDelta { .. } => {}
        AgentEvent::ToolCallRequested { .. } => {}
        AgentEvent::ToolExecutionCompleted { .. } => {}
        AgentEvent::Completed { reply } => {}
        AgentEvent::Failed { error } => {}
    }
}
```

Additional subscribers can observe the same agent session:

```rust
let mut audit_rx = agent.subscribe().await?;

while let Some(event) = audit_rx.recv().await {
    // broadcast consumer
}
```

The crate should also expose a single-turn entrypoint:

```rust
let turn = agent
    .send::<AppMessage, FinalReply, AppToolCall, AppToolResult>(
        AgentInput::Message(message),
    )
    .await?;
```

This is useful for:

- deterministic unit tests
- polling-style integrations
- precise assertions around turn boundaries
- manually driving the agent without spawning a background loop

The agent should be non-generic at the struct level. The typing belongs on the call:

```rust
agent.run::<M, R, C, T>()
```

This matches the direction already taken in `borg-llm`: the runtime object is reusable, while each session chooses the message, response, tool-call, and tool-result types it needs.

### Input Envelope

Inputs to the running agent should be wrapped in a typed envelope:

```rust
pub enum AgentInput<M> {
    Message(M),
    Steer(M),
    Audio(AudioInput),
    Cancel,
}
```

This makes session control explicit and leaves room for:

- normal user/app messages
- steering messages while the model or tools are active
- audio input that can be transcribed through `borg-llm`
- cancellation

### Execution Profiles

The agent should hold a default `ExecutionProfile`, and each run or turn may override it.

```rust
pub struct ExecutionProfile {
    pub model_selector: borg_llm::ModelSelector,
    pub response_mode: borg_llm::ResponseMode,
    pub token_limit: borg_llm::TokenLimit,
    pub temperature: borg_llm::Temperature,
    pub top_p: borg_llm::TopP,
    pub top_k: borg_llm::TopK,
    pub tool_choice: borg_llm::ToolChoice,
    pub repair_policy: RepairPolicy,
}

pub enum RepairPolicy {
    UntilCancelled,
}
```

The crate may also provide presets like:

- `ExecutionProfile::volatile()`
- `ExecutionProfile::deterministic()`

### High-Level Loop

For a running session:

1. The caller starts the agent session with `run()`.
2. The caller sends `AgentInput<MessageType>` values through the input channel.
3. The agent asks `ContextManager` to decode/store those inputs.
4. The agent asks `ContextManager` for the next `ContextWindow`.
5. The agent converts that `ContextWindow` plus the active `ExecutionProfile` into a `CompletionRequest`.
6. The agent calls `LlmRunner` in streaming mode.
7. As output events arrive:
   - text/reasoning/output items are streamed outward
   - typed tool calls are accumulated
8. If the model requested tools:
   - execute them through `ToolRunner`
   - validate tool result typing
   - record them through `ContextManager`
   - loop back into `LlmRunner`
9. If the model completed with a final reply:
   - decode/store that reply through `ContextManager`
   - stream a final completion event
10. If the model output does not deserialize into `ResponseType`:
   - feed a structured repair message back into context
   - retry according to `RepairPolicy`

### Turn Machine

The internal engine should be turn-based.

Conceptually:

```rust
loop {
    let outcome = self.run_turn::<M, R, C, T>().await?;
    match outcome {
        TurnOutcome::Completed { .. } => continue,
        TurnOutcome::Idle => wait_for_more_input,
        TurnOutcome::Cancelled => break,
        TurnOutcome::Failed(_) => break,
    }
}
```

This means:

- `run_turn()` is the core orchestration primitive
- `send()` can enqueue one input and drive one turn
- `run()` or `start()` can spawn a background task that repeatedly calls `run_turn()`

This keeps the core logic:

- bounded
- deterministic at the turn level
- easy to unit test

while still allowing a fully autonomous session API on top.

### Why the Generic Split Looks Like This

There are four distinct application types:

- `MessageType`: inbound app-level messages accepted by the agent
- `ResponseType`: final model reply type the caller expects to consume
- `ToolCallType`: typed tool requests emitted by the model
- `ToolResultType`: typed tool results accepted by the model on the next turn

This split matters because tool calls and tool results are not the same type, and neither should be conflated with the user's application message type.

## Reference-Level Explanation

### Core Traits

The crate should define three primary traits.

#### `ContextManager`

`ContextManager` owns conversation state and prompt shaping.

```rust
#[async_trait::async_trait]
pub trait ContextManager: Send + Sync {
    async fn accept_input<M>(&self, input: AgentInput<M>) -> AgentResult<()>
    where
        M: AgentMessage;

    async fn accept_model_output<R>(&self, output: &TypedAgentOutput<R>) -> AgentResult<()>
    where
        R: AgentResponse;

    async fn accept_tool_result<T>(&self, result: ToolResultEnvelope<T>) -> AgentResult<()>
    where
        T: AgentToolResult;

    async fn build_context_window(&self) -> AgentResult<ContextWindow>;
}
```

The important point is not the exact method names; it is the ownership boundary:

- application/domain messages come in
- a `ContextWindow` describing relevant context comes out
- context shaping and truncation happen here, not in `Agent`

The context manager should maintain its own internal state and not require the caller to keep a separate transcript copy.

`ContextManager` should be allowed to be:

- in-memory only
- database-backed
- compaction-aware
- token-accounting-aware

It may eventually support branching via methods like `.fork()`, but branching is not required for v1.

#### `ContextProvider`

`ContextManager` should be able to aggregate context from multiple providers:

```rust
#[async_trait::async_trait]
pub trait ContextProvider: Send + Sync {
    async fn provide(&self) -> AgentResult<Vec<ContextChunk>>;
}
```

This allows composition of:

- static text/instructions
- dynamic environment snapshots
- memory retrieval
- durable transcript sources

The `ContextManager` remains the owner of selection, tracking, and compaction policy.

#### `ToolRunner`

`ToolRunner` owns typed tool execution.

```rust
#[async_trait::async_trait]
pub trait ToolRunner: Send + Sync {
    async fn run<C, T>(&self, call: ToolCallEnvelope<C>) -> AgentResult<ToolResultEnvelope<T>>
    where
        C: AgentToolCall,
        T: AgentToolResult;
}
```

The runner must enforce:

- the incoming typed tool call is valid
- the produced typed tool result is valid
- errors remain explicit and typed

`ToolRunner` should not know about provider message formats.

#### `AgentMessage`, `AgentResponse`, `AgentToolCall`, `AgentToolResult`

These are marker-plus-conversion traits for the application boundary:

```rust
pub trait AgentMessage: Send + Sync + 'static {
    fn into_input_items(self) -> AgentResult<Vec<borg_llm::InputItem>>;
}

pub trait AgentResponse:
    serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + Send + Sync + 'static
{
}

pub trait AgentToolCall: borg_llm::TypedTool {}

pub trait AgentToolResult:
    serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema + Send + Sync + 'static
{
    fn into_tool_result_items(
        self,
        call_id: &str,
        tool_name: &str,
    ) -> AgentResult<Vec<borg_llm::InputItem>>;
}
```

The exact conversion hooks may evolve, but the contract should be:

- application messages are converted into `borg-llm` input items
- final responses are schema-capable and deserializable
- tool call typing comes from `borg-llm::TypedTool`
- tool results have an explicit conversion back into input items

### Context Window

`ContextManager` should return a neutral context description:

```rust
pub struct ContextWindow {
    pub system_items: Vec<borg_llm::InputItem>,
    pub conversation_items: Vec<borg_llm::InputItem>,
    pub tool_result_items: Vec<borg_llm::InputItem>,
    pub metadata: ContextWindowMetadata,
}
```

The agent then combines:

- `ContextWindow`
- `ExecutionProfile`
- `ToolCallType`
- `ResponseType`

to construct the actual `borg_llm::CompletionRequest`.

This keeps policy application in the agent and context selection in the context manager.

### Agent Loop Contract

`Agent::run()` should:

1. start a background session loop
2. receive `AgentInput<MessageType>` values from the input stream
3. accept those inputs into context
4. build an LLM request from `ContextWindow + ExecutionProfile`
5. call `LlmRunner::chat_stream::<ToolCallType, ResponseType>(...)`
6. stream normalized events
7. collect tool calls from the streamed/final output
8. execute tools through `ToolRunner`
9. feed tool results into context
10. if final output does not deserialize into `ResponseType`, inject a repair message and retry according to `RepairPolicy`
11. repeat until a valid final assistant reply is produced, the session is cancelled, or an unrecoverable error occurs

This produces a loop with explicit control flow and strong subsystem boundaries.

### Turn Contract

`Agent::run_turn()` should:

1. consume the next available input or pending work
2. accept that input into context
3. build an LLM request from `ContextWindow + ExecutionProfile`
4. run one bounded model/tool/repair cycle until the turn reaches a stable outcome
5. emit turn-scoped events
6. return a `TurnOutcome`

`Agent::send()` should be a thin helper that:

1. enqueues one `AgentInput<MessageType>`
2. drives one `run_turn()`
3. returns the turn-scoped handle or outcome

`Agent::run()` should be a thin helper that:

1. spawns a background task
2. repeatedly calls `run_turn()`
3. reacts to `TurnOutcome` values to continue, idle, cancel, or stop

### Event Protocol

`AgentEvent` should be stable and intentionally small:

```rust
pub enum AgentEvent<M, R, C, T> {
    InputAccepted { input: AgentInput<M> },
    TurnStarted { turn: u64 },
    ModelOutputDelta { delta: borg_llm::OutputItemDelta },
    ModelOutputItem { item: borg_llm::OutputItem },
    ToolCallRequested { call: ToolCallEnvelope<C> },
    ToolExecutionStarted { call: ToolCallEnvelope<C> },
    ToolExecutionCompleted { result: ToolResultEnvelope<T> },
    TurnCompleted { turn: u64 },
    Completed { reply: R },
    Failed { error: AgentError },
}
```

Important constraints:

- events should be derived from the actual loop lifecycle
- events should not expose provider-specific wire details
- streamed deltas and completed items should both be representable

### Envelope Types

Tool calls and tool results should carry stable metadata:

```rust
pub struct ToolCallEnvelope<C> {
    pub call_id: String,
    pub tool_name: String,
    pub call: C,
}

pub struct ToolResultEnvelope<T> {
    pub call_id: String,
    pub tool_name: String,
    pub result: ToolExecutionResult<T>,
}

pub enum ToolExecutionResult<T> {
    Ok(T),
    Error { message: String },
}
```

This keeps the loop explicit about:

- which tool call a result belongs to
- whether the tool failed or succeeded
- what gets fed back to context on the next turn

### Error Model

`borg-agent` should preserve `borg-llm`'s typed error semantics rather than flattening them.

At minimum:

```rust
pub enum AgentError {
    InvalidMessage { reason: String },
    Context { reason: String },
    Llm(borg_llm::Error),
    ToolDecode { tool_name: String, reason: String },
    ToolExecution { tool_name: String, reason: String },
    ToolResultEncode { tool_name: String, reason: String },
    Cancelled,
}
```

This is important because callers may want to make policy decisions based on:

- provider HTTP failures
- malformed model output
- bad tool calls
- tool execution failures

### Internal State Machine

The agent should remain explicit rather than callback-heavy:

```text
Idle
  -> AwaitInput
  -> AcceptInput
  -> BuildContextWindow
  -> SampleModel
  -> StreamOutputs
     -> ExecuteTools -> RecordToolResults -> BuildContextWindow -> SampleModel
     -> InvalidReply -> RecordRepairMessage -> SampleModel
     -> FinalReply -> RecordReply -> Complete
     -> Cancel -> Fail
     -> Error -> Fail
```

This should live as ordinary Rust control flow, not an abstract state machine framework.

The per-turn boundary should be explicit in code and in tests.

### Turn Outcome

The turn machine should return a small outcome enum:

```rust
pub enum TurnOutcome<R> {
    Completed { reply: R },
    Idle,
    Cancelled,
    Failed(AgentError),
}
```

This gives `run()` a trivial control loop while also making `run_turn()` and `send()` easy to test in isolation.

## Design Decisions

### 1. `Agent` is non-generic

This matches `LlmRunner` and keeps one runtime object reusable across different typed calls.

### 2. Context ownership is first-class

The loop should not manipulate raw message vectors directly as its source of truth.

### 3. The loop is session-oriented

One `Agent` instance represents one long-lived session. Inputs and outputs are streamed through channels.

### 4. The internal engine is turn-based

The long-lived session API should be implemented as a thin loop over `run_turn()`.

### 5. The loop is explicit and readable

There should be one clearly understandable orchestration loop.

### 6. `borg-agent` stays item-based

This follows the new `borg-llm` contract. The crate should not reintroduce provider-facing chat message abstractions.

### 7. Tool results are typed separately from tool calls

This avoids the old `borg-agent` design smell where tool handling leaned too hard on JSON conversion and a single "tool type" mindset.

### 8. Invalid `ResponseType` is a repair path, not an immediate failure

The agent should feed validation errors back into the model and retry according to `RepairPolicy`.

## Testing Strategy

The crate should be designed for heavy unit testing with fake subsystems.

We should provide fake:

- `LlmRunner`
- `ContextManager`
- `ToolRunner`

The important tests are not provider e2e tests. They are behavior tests for:

- model emits final response immediately
- model emits one tool call, tool succeeds, next turn completes
- model emits multiple tool calls in one turn
- model emits invalid typed tool call
- tool execution fails and error result is fed back
- model emits output that does not match `ResponseType`
- model emits output that does not match `ResponseType` and the agent repairs successfully
- streamed deltas and final items produce the expected event order
- context manager is called in the expected sequence
- `Cancel` input interrupts the active session
- `subscribe()` observers receive the same event stream as the primary receiver
- `send()` drives exactly one turn
- `run()` is behaviorally equivalent to repeatedly calling `run_turn()`

## Drawbacks

- there are several generic type parameters on `run()`
- defining the conversion boundary for `MessageType -> InputItem` and `ToolResultType -> InputItem` needs care
- a very small initial design may need to grow later for approvals or persistence hooks

## Alternatives

### Reuse `crates.old/borg-agent`

Rejected. The types and boundaries are wrong for the new `borg-llm` architecture.

### Put the loop inside `borg-llm`

Rejected. `borg-llm` should remain an LLM transport and typing layer, not an agent orchestration crate.

### Make `Agent<M, R, C, T>` generic at the struct level

Rejected. That makes the runtime less reusable and couples one agent instance to one schema universe.

## Unresolved Questions

1. Should v1 expose per-turn `ExecutionProfile` overrides through the input stream, or only via agent/session configuration?
2. How should the model request parallel tool execution in the normalized item/output contract?
3. Should `ContextProvider` be pull-only in v1, or should providers also be able to subscribe to context updates?
4. What exact shape should repair messages take when `ResponseType` validation fails?
5. How much of the `AgentEvent` protocol should be persisted by callers vs treated as ephemeral UI output?

## Rollout Plan

1. Land `borg-agent` crate with core traits and fake-driven unit tests.
2. Implement a minimal in-memory `ContextManager`.
3. Implement `ContextProvider` support in that manager.
4. Implement a registry-backed `ToolRunner`.
5. Implement `run_turn()` as the core engine.
6. Implement `send()` as a single-turn driver.
7. Implement the session-oriented `run()` loop over `run_turn()`.
8. Add one integration test using real `borg-llm` and fake tools.
