# RFD0031 Implementation Checklist (Hard Cut)

This checklist turns [RFD0031-structured-builtin-tool-calls.md](./RFD0031-structured-builtin-tool-calls.md) into concrete code work.

Scope: built-in tool calls/results only. MCP wire semantics are out of scope.
Mode: destructive/hard-cut. No backward compatibility layer.

##Target state
1. Built-in tool result payloads remain structured from tool execution through persistence and runtime eventing.
2. Provider boundaries serialize deterministically (JSON), not prose wrappers.
3. Built-in tools stop returning JSON-as-string for machine payloads.

##Workstream 1: Canonical tool output shape (`borg-agent`)
Files:
- `crates/borg-agent/src/tools.rs`
- `crates/borg-agent/src/message.rs`
- `crates/borg-agent/src/agent.rs`
- `crates/borg-agent/src/actor_thread.rs`
- `crates/borg-agent/src/lib.rs`

Tasks:
1. Replace `ToolResultData<T>` variants (`Text/Capabilities/Execution/Error`) with one canonical envelope type.
2. Update `ToolResponse<T>` to carry the canonical envelope.
3. Add constructors/helpers for tool authors (for example `ok(result)`, `error(code, message)`, `with_duration_ms`).
4. Update `Message::ToolResult`, `ToolCallRecord`, and actor run output to use the new shape.
5. Update `ActorThread::record_tool_call` success/error/duration extraction to read from the new envelope.

Acceptance criteria:
1. No runtime code path depends on `ToolResultData::Text` for machine payloads.
2. Tool call persistence still records `success`, `error`, `duration_ms`, and full `output_json`.

##Workstream 2: Adapter boundary and provider message model
Files:
- `crates/borg-agent/src/llm_adapter.rs`
- `crates/borg-llm/src/types.rs`
- `crates/borg-llm/src/providers/openai.rs`
- `crates/borg-llm/src/providers/openrouter.rs`

Tasks:
1. Remove prose flattening (`tool_result_to_text`, `"execution result in ..."`/`"tool error: ..."`) from `llm_adapter`.
2. Change tool-result mapping to deterministic JSON serialization of canonical output.
3. Update provider message type if needed to carry structured tool-result payload up to adapter boundary.
4. In provider adapters, emit `role=tool` content as minified JSON string when provider requires text.

Acceptance criteria:
1. No English prose wrapper is added by runtime for tool results.
2. Same logical tool result serializes identically across runs (stable key ordering where applicable).

##Workstream 3: Built-in tool implementations stop JSON-string output
Primary files:
- `crates/borg-memory/src/tools.rs`
- `crates/borg-taskgraph/src/tools.rs`
- `crates/borg-schedule/src/tools.rs`
- `crates/borg-apps/src/discovery.rs`
- `crates/borg-fs/src/lib.rs`
- `crates/borg-ports-tools/src/lib.rs`
- `crates/borg-agent/src/admin_tools.rs`
- `crates/borg-exec/src/tool_runner.rs`

Tasks:
1. Replace helper functions like `json_text(...)` that currently return `ToolResultData::Text(serde_json::to_string(...))`.
2. Return structured JSON values directly in canonical output envelope.
3. Keep plain text only for genuinely human-textual tools (not object payloads).

Acceptance criteria:
1. `rg "ToolResultData::Text\\(serde_json::to_string|json_text\\(" crates` returns no built-in machine payload paths.
2. Actors tools (`Actors-sendMessage`, `Actors-receive`, provider admin tools) return structured objects, not embedded JSON strings.

##Workstream 4: Stage/GraphQL projection updates
Files:
- `apps/stage/src/App.tsx`
- `crates/borg-gql/src/sdl/mod.rs`
- `crates/borg-gql/src/lib.rs` (runtime HTTP response projection)

Tasks:
1. Ensure Stage mailbox tool rendering prefers canonical keys (`ok`, `result`, `error`, `meta`) and still handles old rows after DB reset.
2. Ensure GraphQL typed projections do not assume `payload.content` is always a string for tool-result messages.
3. Keep bundled tool call/result rendering in Stage with new structured outputs.

Acceptance criteria:
1. Stage shows tool results as fielded JSON without `(non-text payload)` placeholders.
2. Tool call/result grouping still works after shape migration.

##Workstream 5: DB/data hard cut
Files:
- `crates/borg-db/migrations/*` (new migration)
- optional operator/dev reset script docs

Tasks:
1. Add destructive migration if schema needs to change (or keep schema and only hard-reset rows).
2. Clear legacy rows that contain old flattened tool-result payloads.
3. Keep `tool_calls.output_json` as canonical stored structured payload.

Acceptance criteria:
1. Local/dev DB has no legacy tool-result rows requiring compatibility logic.
2. `tool_calls` table contains canonical envelope payloads only.

##Workstream 6: Tests and fixtures
Files:
- `crates/borg-agent/src/tests.rs`
- `crates/borg-agent/tests/agentic_loop_with_real_llm.rs`
- `crates/borg-memory/tests/rfd0005_property_suite.rs`
- `crates/borg-memory/tests/rfd0005_test_suite.rs`
- `crates/borg-taskgraph/src/tests.rs`

Tasks:
1. Replace assertions expecting `ToolResultData::Text("...json...")`.
2. Add tests for canonical structured output serialization and adapter conversion.
3. Add regression test: interrupted tool call yields structured error output.

Acceptance criteria:
1. No tests rely on prose tool-result wrappers.
2. Provider-message conversion tests confirm tool results are deterministic JSON payloads.

##Destructive cleanup commands (dev/local)
Run after migration lands:

```sh
sqlite3 "$HOME/.borg/config.db" "BEGIN; DELETE FROM messages; DELETE FROM tool_calls; COMMIT;"
```

If needed, also clear cursor/state tables used by mailbox replay in your environment.

##Validation gates
1. `cargo build`
2. `cargo test -p borg-agent -p borg-exec -p borg-gql -p borg-memory -p borg-taskgraph`
3. `bun run build:web`
4. Manual smoke:
- Stage mailbox shows structured tool outputs.
- Actor-to-actor send/receive tools still function.
- `/ports/http` response includes tool calls with canonical output shape.

##Suggested execution order
1. Workstream 1
2. Workstream 2
3. Workstream 3
4. Workstream 6
5. Workstream 4
6. Workstream 5 + cleanup + final verification
