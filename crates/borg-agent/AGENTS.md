# borg-agent

Session-first typed agent runtime over `borg-llm`.

## Structure

```
src/
├── agent.rs      # AgentBuilder, Agent, run loop, turn state machine
├── context.rs    # ContextManager, ContextChunk, ContextProvider
├── storage.rs    # StorageAdapter and storage event model
├── tools.rs      # ToolRunner plus typed tool envelopes/results
└── error.rs      # AgentError and AgentResult
```

## Key Contracts

### Agent typing
- `Agent<M, C, T, R>` is the concrete typed runtime.
- The agent type itself now carries the same bounds the runtime needs:
  - `M: Into<InputItem>`
  - `C: TypedTool + Clone + Serialize`
  - `T: Clone + Serialize`
  - `R: Clone + Serialize + DeserializeOwned + JsonSchema`
- Keep those constraints aligned with the actual `send`, `next`, and `run` requirements.

### Turn ownership
- `TurnState` owns the pending-tool bookkeeping.
- If cancel/steer behavior changes, update the `TurnState` methods rather than adding new free helper functions around it.
- `ToolCallEnvelope` and `ToolResultEnvelope` own their conversion into `ContextChunk`s.

### Event order
- Preserve this ordering invariant:
  - model output items first
  - tool call request events before tool execution completion
  - final `Completed` or `Cancelled` event last
- If you change turn execution, update the unit tests in `agent.rs` that assert event sequencing and transcript reuse.

### Context model
- `ContextManager` history only contains session history.
- Provider chunks come from `ContextProvider`s and are prepended at window materialization time.
- Do not mix provider state into stored history.

## Commands

```bash
cargo build -p borg-agent
cargo test -p borg-agent
```

Provider-backed e2e tests live under `tests/` and are slower than the unit suite in `src/agent.rs`.
