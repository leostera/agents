# borg-agent

Session-first typed agent runtime over `borg-llm`.

## Structure

```
src/
├── agent/
│   ├── mod.rs    # Agent trait plus shared spawn/call/cast/steer/cancel helpers
│   └── session.rs # AgentBuilder, SessionAgent, turn state machine, spawn loop
├── context.rs    # ContextManager, ContextChunk, ContextProvider
├── storage.rs    # StorageAdapter and storage event model
├── tools.rs      # ToolRunner plus typed tool envelopes/results
└── error.rs      # AgentError and AgentResult
```

## Key Contracts

### Agent typing
- `Agent` is the shared trait boundary used by app code, wrappers, and evals.
- `SessionAgent<M, C, T, R>` is the built-in concrete typed runtime.
- Keep the trait-associated-type contract aligned with the actual runtime needs:
  - `Input: Clone + Serialize + DeserializeOwned`
  - `ToolCall: Clone + Serialize + DeserializeOwned`
  - `ToolResult: Clone + Serialize + DeserializeOwned`
  - `Output: Clone + Serialize + DeserializeOwned + JsonSchema`
- Preserve the layering:
  - `send` + `next` are the semantic core
  - `cast`, `call`, `steer`, `cancel`, and `spawn` are convenience/default surfaces

### Turn ownership
- `TurnState` owns the pending-tool bookkeeping.
- If cancel/steer behavior changes, update the `TurnState` methods rather than adding new free helper functions around it.
- `ToolCallEnvelope` and `ToolResultEnvelope` own their conversion into `ContextChunk`s.

### Event order
- Preserve this ordering invariant:
  - model output items first
  - tool call request events before tool execution completion
  - final `Completed` or `Cancelled` event last
- If you change turn execution, update the unit tests in `src/agent/session.rs` that assert event sequencing and transcript reuse.

### Context model
- `ContextManager` history only contains session history.
- Provider chunks come from `ContextProvider`s and are prepended at window materialization time.
- Do not mix provider state into stored history.

## Commands

```bash
cargo build -p borg-agent
cargo test -p borg-agent
```

Provider-backed e2e tests live under `tests/` and are slower than the unit suite under `src/agent/`.
