# RFD0051 - Composable Agents

- Feature Name: `composable-agents`
- Start Date: `2026-03-18`
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary

Unify the current `borg-evals`-local `EvalAgent` concept with the main `borg-agent` runtime around one shared trait: `trait Agent`. The built-in concrete runtime becomes `SessionAgent<M, C, T, R>`, and both evals and application code depend on the shared trait instead of a parallel eval-only abstraction.

## Motivation

Today we have two concepts for “something that behaves like an agent”:

- `borg_agent::Agent<M, C, T, R>`
- `borg_evals::EvalAgent`

That split is workable, but it is bad DX and it fights the structure the code already has.

The current `borg-agent` implementation in `crates/borg-agent/src/agent.rs` already has a clear semantic center:

- `send(&mut self, AgentInput<M>)`
- `next(&mut self)`
- `run(self)` as a spawned adapter over the same turn machine

So the real problem is not missing semantics. The problem is that the shared agent abstraction does not exist at the crate boundary.

We want:

- application-defined structs to be real agents
- wrappers like instrumentation/logging/retries to also be agents
- evals to work against the same abstraction as production code
- judges later to also be ordinary agents

## Guide-level explanation

The final public model should be:

- `trait Agent`
- `struct SessionAgent<M, C, T, R>`

`Agent` is the shared typed contract. `SessionAgent` is one implementation of that contract.

Application code should look like this:

```rust
#[derive(borg_macros::Agent)]
pub struct EchoAgent {
    #[agent]
    inner: SessionAgent<EchoReq, EchoTool, String, EchoRes>,
}
```

That means:

- your domain struct is the agent
- the built-in runtime stays hidden inside it
- evals can depend on `EchoAgent` directly
- wrappers can compose normally:
  - `Instrumented<EchoAgent>`
  - `Retried<EchoAgent>`
  - `Logged<EchoAgent>`

Turn-based usage remains the primitive:

```rust
agent.cast(req).await?;
while let Some(event) = agent.next().await? {
    // inspect events
}
```

Streaming remains available:

```rust
let (tx, rx) = agent.run().await?;
```

But that streaming API should be defined in terms of the turn API, not as a separate implementation path.

The trait should also provide convenience helpers:

- `cast(message)` for fire-and-forget message enqueueing
- `call(message)` for one-turn request/response execution
- `steer(message)` for sending steering input and awaiting the resulting turn outcome
- `cancel()` for requesting cancellation and awaiting cancellation to be observed

So contributors can choose the right level:

- `send` / `next` for precise control
- `cast` / `call` / `steer` / `cancel` for common turn operations
- `run` for streaming integration

## Reference-level explanation

Introduce a new trait in `borg-agent`:

```rust
#[async_trait]
pub trait Agent: Send + 'static {
    type Input;
    type ToolCall;
    type ToolResult;
    type Output;

    async fn send(&mut self, input: AgentInput<Self::Input>) -> AgentResult<()>;

    async fn next(
        &mut self,
    ) -> AgentResult<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>>;

    async fn cast(&mut self, input: Self::Input) -> AgentResult<()>;

    async fn call(
        &mut self,
        input: Self::Input,
    ) -> AgentResult<Self::Output>;

    async fn steer(
        &mut self,
        input: Self::Input,
    ) -> AgentResult<Option<Self::Output>>;

    async fn cancel(&mut self) -> AgentResult<()>;

    async fn run(
        self,
    ) -> AgentResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )>
    where
        Self: Sized;
}
```

`send` and `next` are the semantic core. The others should be default implementations:

- `cast` delegates to `send(AgentInput::Message(...))`
- `call` sends a normal message and then drives `next()` until the turn completes
- `steer` sends a steering message and then drives `next()` until the turn resolves
- `cancel` sends `AgentInput::Cancel` and then drives `next()` until cancellation is observed
- `run(self)` remains a spawned adapter over `send` and `next`

The current `borg_agent::Agent<M, C, T, R>` should be renamed to `SessionAgent<M, C, T, R>` and should implement the trait directly.

The default implementation of `run(self)` should stay a thin spawned adapter around `send` and `next`, because that is already how the runtime works today.

`borg-evals` should then:

- remove `EvalAgent`
- depend on `borg_agent::Agent`
- update derives/macros to implement the shared trait

Instrumentation should wrap the trait, not special-case evals:

```rust
pub struct Instrumented<A> {
    inner: A,
    sink: Arc<dyn AgentObserver>,
}
```

If `Instrumented<A>` implements `Agent` by forwarding `send` and `next`, then both:

- turn-based tests
- streaming execution

get the same instrumentation behavior automatically.

## Drawbacks

- this is a breaking rename from `Agent<...>` to `SessionAgent<...>`
- `borg-evals` and macros will need coordinated migration
- the trait name `Agent` commits us to this concept as the main public boundary

## Rationale and alternatives

The main alternative is to keep `EvalAgent` and accept two agent abstractions.

That is worse because:

- wrappers stay awkward
- judges become special instead of ordinary agents
- evals keep depending on a separate contract from production code

Another alternative is to make `run(self)` the only shared trait method.

That is worse because:

- it hides the real primitive
- wrappers become harder to write
- turn-based testing becomes a second-class path

The current runtime already shows the right layering: turn machine first, stream adapter second.

## Unresolved questions

- Should `run(self)` be a required trait method or a provided default implementation?
- Should the derive macro be named `#[derive(Agent)]` immediately, or migrate in two steps?
- Do we want an additional erased/object-safe adapter later, or is the typed trait enough for now?

## Future possibilities

- judge agents become ordinary `Agent` implementations
- eval instrumentation can wrap agents instead of inventing eval-local adapters
- richer decorators become normal Rust composition instead of framework-specific hooks
