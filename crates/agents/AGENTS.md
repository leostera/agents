# agents

Typed Rust APIs for LLM providers, sessions, tools, context, and storage.

## Structure

```
src/
├── lib.rs           # Public crate entrypoint plus re-exports
├── llm/             # Provider-neutral completions, transcription, runner, providers
└── agent/           # Agent trait, SessionAgent, context, storage, tool execution
build.rs             # macOS Swift package linkage for Apple transcription provider
swift/               # Apple Speech bridge sources
```

## Key Contracts

### LLM layer
- `LlmRunner` is provider-neutral and not generic.
- Type parameters live on each completion request/response call.
- Providers only translate between provider wire formats and the shared raw request/response types.
- Keep `ModelSelector`, tuning knobs, and tool/response typing provider-neutral.

### Agent typing
- `Agent` is the shared trait boundary used by app code, wrappers, and evals.
- `SessionAgent<M, C, T, R>` is the built-in concrete typed agent.
- Preserve the associated-type contract:
  - `Input: Clone + Serialize + DeserializeOwned`
  - `ToolCall: Clone + Serialize + DeserializeOwned`
  - `ToolResult: Clone + Serialize + DeserializeOwned`
  - `Output: Clone + Serialize + DeserializeOwned + JsonSchema`

### Turn ownership
- `TurnState` owns pending-tool bookkeeping.
- `ToolCallEnvelope` and `ToolResultEnvelope` own their conversion into `ContextChunk`s.
- Keep the layering intact:
  - `send` + `next` are the semantic core
  - `cast`, `call`, `steer`, `cancel`, and `spawn` are convenience/default surfaces

### Event order
- Preserve this ordering invariant:
  - model output items first
  - tool call request events before tool execution completion
  - final `Completed` or `Cancelled` event last

### Context model
- `ContextManager` history only contains session history.
- Provider chunks come from `ContextProvider`s and are prepended at window materialization time.
- Do not mix provider state into stored history.

### Testing helpers
- `llm/testing` is test support, not normal app runtime surface.
- The testing helpers should not install tracing subscribers; callers own logging configuration.

## Commands

```bash
cargo build -p agents
cargo test -p agents
```
