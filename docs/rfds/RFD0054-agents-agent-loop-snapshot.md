# RFD0054 - `agents` Agent Loop Snapshot

- Feature Name: `agents-agent-loop-snapshot`
- Start Date: `2026-03-20`
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary

Document the current `agents` runtime loop as it exists today in
`crates/agents/src/agent/runtime/session.rs`.

This is a snapshot RFD, not a redesign.

It explains:

- the public surface of `SessionAgent<M, C, T, R>`
- how `send`, `next`, and `spawn` relate
- how turns are queued, started, cancelled, and steered
- how the loop materializes context, calls the model, executes tools, and emits events
- what invariants application code and evals can rely on today

## Motivation

The runtime has crossed the threshold where “just read the code” is no longer a
reasonable explanation of how the agent loop works.

There are now several moving parts that matter together:

- `LlmRunner`
- `ContextManager`
- `ToolRunner`
- `StorageAdapter`
- typed model input/output
- turn state
- queued turns
- event ordering
- cancellation and steering semantics
- wasm vs native `spawn` behavior

We already have older design RFDs for the typed loop and composable agents:

- `RFD0049 - Typed Agent Loop for borg-agent`
- `RFD0051 - Composable Agents`

Those are useful design history, but they no longer describe the current
runtime exactly. We need one document that says, plainly:

- what the loop is
- what state it owns
- what it emits
- what it guarantees

## Non-Goals

This document does not propose:

- a new event schema
- a new cancellation model
- a new tool contract
- object-safe agent traits
- distributed agent execution
- approval or sandbox integration

It only captures the current runtime.

## Guide-Level Explanation

### What a `SessionAgent` is

`SessionAgent<M, C, T, R>` is the built-in model-backed implementation of
`trait Agent`.

It owns:

- an `Arc<LlmRunner>`
- a `ContextManager`
- an `ExecutionProfile`
- a `ToolRunner<C, T>`
- a `StorageAdapter`
- mutable turn/session state

The four type parameters are:

- `M`: application input message type
- `C`: typed tool call enum
- `T`: typed tool result payload
- `R`: final reply type

At the application boundary, that means you can choose:

- string-in, string-out agents
- typed request / typed response agents
- no-tool agents
- tool-using agents with typed calls and typed results

### The semantic core: `send` and `next`

The runtime is a turn machine.

The semantic core is:

```rust
agent.send(input).await?;
let event = agent.next().await?;
```

Everything else is built around that model:

- `call(...)` sends one input and drives `next()` until completion
- `cast(...)` is a convenience wrapper over sending a normal message
- `steer(...)` is a convenience wrapper over sending steering input
- `cancel(...)` is a convenience wrapper over sending a cancellation request
- `spawn(...)` is a streaming adapter over the same `send` + `next` loop

That layering matters.

It means there is one actual agent loop, not separate “manual” and “streaming”
implementations.

### Inputs

Inputs are explicit:

```rust
pub enum AgentInput<M> {
    Message(M),
    Steer(M),
    Cancel,
}
```

The semantics are:

- `Message(M)`:
  - starts a new turn if the agent is idle
  - otherwise queues a future turn
- `Steer(M)`:
  - if the agent is active, interrupts remaining pending tool work and pushes
    the steering message into the current conversation state
  - if the agent is idle, starts a new turn
- `Cancel`:
  - if the agent is active, marks the turn as cancellation-pending
  - if idle, it is effectively a no-op after recording the input

### Outputs

The runtime emits typed `AgentEvent<C, T, R>` values:

- `ContextWindowMaterialized { window }`
- `RequestPrepared { request }`
- `ModelOutputItem { item, usage_metrics }`
- `ToolCallRequested { call, usage_metrics }`
- `ToolExecutionCompleted { result }`
- `Completed { reply, usage_metrics }`
- `Cancelled`

These are the only semantic events application code, wrappers, storage, and
evals need to understand.

### Event ordering

The event ordering invariant is:

1. materialized context window first, when one should be emitted
2. prepared request summary
3. zero or more model output items
4. zero or more tool call request events
5. zero or more tool execution completion events
6. final `Completed` or `Cancelled`

More concretely:

- model output items always come before final completion
- a tool call request is emitted before its tool execution completion
- `Completed` and `Cancelled` are terminal events for a turn

This is the contract the eval and transcript layers rely on.

### What “one turn” means

One turn is not just one model request.

One turn may involve:

1. one user or steering input
2. context materialization
3. one model response
4. zero or more tool calls
5. one or more subsequent model requests after tool results are fed back in
6. a final typed reply or a cancellation

The turn boundary is application-visible through:

- `StorageRecord::TurnStarted`
- the active turn id
- the final terminal event

## Reference-Level Explanation

### Public builder surface

`SessionAgent::builder()` starts at the default shape:

```rust
SessionAgent<String, (), (), String>
```

That means:

- string messages
- no typed tools
- no typed tool results
- string replies

The builder can move into stronger typing with:

- `with_message_type::<M2>()`
- `with_response_type::<R2>()`
- `with_tool_runner::<C2, T2, Runner>(...)`

The builder also lets callers provide:

- `with_llm_runner(...)`
- `with_execution_profile(...)`
- `with_context_manager(...)`
- `with_storage_adapter(...)`
- `with_run_channel_capacity(...)`

### Runtime-owned state

`SessionAgent` owns:

- `next_turn: u64`
- `next_response: u64`
- `active_turn: Option<ActiveTurn<C, T, R>>`
- `queued_turns: VecDeque<QueuedTurn>`

The active turn carries:

- the turn id
- the execution profile used for this turn
- a `TurnState<C, T, R>`

Queued turns carry:

- the future turn id
- the profile to use when that turn becomes active
- the already-lowered `InputItem`

### Turn states

The current internal turn machine is:

```rust
enum TurnState<C, T, R> {
    CancelPending,
    NeedLlm,
    DispatchRequest { request: CompletionRequest<C, R> },
    ExecuteTool {
        current: ToolCallEnvelope<C>,
        remaining: VecDeque<ToolCallEnvelope<C>>,
        usage_metrics: UsageMetrics,
    },
    EmitQueue {
        queue: VecDeque<AgentEvent<C, T, R>>,
        next: Box<TurnState<C, T, R>>,
    },
    Done,
}
```

This is the actual loop.

The main transitions are:

- `NeedLlm`
  - build the next request from the current context window
  - queue `ContextWindowMaterialized` and `RequestPrepared`
  - transition into `DispatchRequest`
- `DispatchRequest`
  - call `LlmRunner::chat::<C, R>(...)`
  - decode the response into:
    - streamed output events
    - tool call execution
    - or final completion
- `ExecuteTool`
  - run one typed tool call through the configured `ToolRunner`
  - push the resulting tool result into context
  - emit `ToolExecutionCompleted`
  - either execute the next tool or return to `NeedLlm`
- `EmitQueue`
  - drain already-computed events one by one
- `CancelPending`
  - emit `Cancelled`
- `Done`
  - terminal internal state

### Request construction

`build_request(...)` does the following:

1. materializes the current `ContextWindow` from `ContextManager`
2. lowers it into provider-neutral `InputItem`s
3. builds a `CompletionRequest<C, R>`
4. applies the active `ExecutionProfile`
5. conditionally attaches tool schemas if `C != ()`
6. conditionally attaches a typed response format if `R != String`

The request is always issued in:

- `ResponseMode::Buffered`

The streaming behavior at the agent layer comes from how the buffered
`CompletionResponse` is decomposed into `AgentEvent`s, not from the model call
itself running in a streaming wire protocol.

### Tool calls

Tool calls are extracted from `CompletionResponse<C, R>` as typed
`OutputItem::ToolCall { call }` values.

Each tool call is wrapped as a `ToolCallEnvelope<C>` containing:

- `call_id`
- `name`
- raw JSON arguments
- typed decoded `call`

The runtime then:

1. pushes each requested call into context as a compactable chunk
2. emits `ToolCallRequested`
3. executes the current tool call through `ToolRunner<C, T>`
4. wraps the result as a `ToolResultEnvelope<T>`
5. pushes the result into context
6. emits `ToolExecutionCompleted`

If more tool calls remain from the same response, the runtime continues through
them before returning to the model.

### Usage metrics

Every model response gets a `UsageMetrics` payload containing:

- a monotonically increasing `response_id`
- `provider`
- `model`
- `finish_reason`
- provider usage counts

Those usage metrics are attached to the response-derived events:

- `ModelOutputItem`
- `ToolCallRequested`
- `Completed`

This lets evals and reports aggregate usage at the transcript level without
inventing a separate usage event stream.

### Context behavior

`ContextManager` owns session history.

The runtime pushes into it:

- normal user messages
- steering messages
- tool calls
- tool results
- final replies

Provider or static context is not mixed directly into stored history. Instead,
it is materialized when a `ContextWindow` is requested.

That is why the runtime can emit:

- `ContextWindowMaterialized { window }`

without treating provider context as ordinary session history.

### Storage behavior

`StorageAdapter` records:

- received inputs
- turn start and turn queueing
- emitted semantic events

This is intentionally driven from the same semantic loop as application code.
Storage is not a separate hidden control path.

### Cancellation and steering

Cancellation and steering both operate against the current active turn state.

For cancellation:

- the runtime records the cancellation input
- any still-pending tool calls are converted into abandoned tool results with
  `"cancelled"`
- the active turn transitions to `CancelPending`
- the next `next()` call emits `Cancelled`

For steering:

- if a turn is active, pending tool calls are abandoned with
  `"interrupted by steering"`
- the steering message is pushed into conversation context
- the active turn returns to `NeedLlm`
- the next model request is built from the updated conversation

This means steering is not a side channel. It becomes part of the actual
conversation state before the next request is built.

### Queueing semantics

The runtime only has one active turn at a time.

If a normal message arrives while a turn is active:

- a new turn id is reserved
- the input is recorded
- the input is queued in `queued_turns`

When `next()` observes no active turn, it starts the next queued turn
automatically.

So queueing is:

- explicit
- ordered
- local to the session

### Native vs wasm `spawn`

On native targets:

- `spawn(self)` creates a pair of mpsc channels
- it runs a background task that forwards `send(...)` and `next(...)`
- the spawned loop is therefore just an adapter over the same turn machine

On `wasm32`:

- `spawn(self)` currently returns a typed internal error
- the runtime does not expose the spawned adapter there

This is a deliberate current limitation of the implementation, not a separate
agent model.

## Invariants

The current runtime guarantees:

- one active turn at a time
- queued turns preserve FIFO order
- model output items are emitted before final completion
- tool call requests are emitted before their execution completion
- completion or cancellation is terminal for a turn
- typed tool calls and typed replies are validated at the runtime boundary
- session history is owned by `ContextManager`
- materialized provider context is emitted separately from session history

## Known Limitations

- `spawn()` is not supported on `wasm32`
- the agent loop currently uses buffered model responses internally rather than
  a wire-level streaming provider path
- tool interruption is represented by synthetic abandoned tool results
- the event schema does not yet expose every possible low-level provider detail

## Rationale and Alternatives

The key design choice is that the semantic center remains:

- `send`
- `next`

The main alternative would be to make the spawned streaming API the primary
runtime model.

That would be worse because:

- turn-level unit tests become harder to write
- wrappers would need to reimplement stream orchestration
- evals would have to depend on a background-task path instead of the semantic
  turn machine

The current implementation is better because the spawned interface is thin and
the turn machine is explicit.

## Future Work

This snapshot should be revised if we change:

- the event schema substantially
- the turn state machine
- the cancellation model
- the model execution path from buffered to streamed at the runtime core
- wasm spawn behavior

The next design RFD in this area should build on this snapshot rather than
re-explaining the current implementation from scratch.
