# RFD0031 Implementation Checklist (Hard Cut)

This checklist operationalizes [RFD0031-structured-builtin-tool-calls.md](./RFD0031-structured-builtin-tool-calls.md).

Mode: hard cut. No backward compatibility layer.
Scope: built-in tools only.

##Target state
1. Runtime tool outputs are structured envelopes, not prose.
2. Provider adapters serialize deterministically and only stringify at final fallback boundary.
3. Built-in tools return structured payloads directly.
4. Stage/GraphQL render structured outputs without `(non-text payload)` placeholders.

##Workstream 0: Baseline and guardrails
Files:
1. `crates/borg-agent/src/tools.rs`
2. `crates/borg-agent/src/message.rs`
3. `crates/borg-agent/src/llm_adapter.rs`

Tasks:
1. Confirm canonical envelope type is available and exported.
2. Add or update type aliases so legacy names do not leak into new code paths.
3. Add temporary search gates to track legacy flattening callsites.

Search gates:
1. `rg "ToolResultData::Text|ToolResultData::Execution|ToolResultData::Capabilities" crates`
2. `rg "execution result in|tool error:" crates/borg-agent crates/borg-exec crates/borg-llm`

Exit criteria:
1. You have a complete list of old-shape callsites to eliminate.

##Workstream 1: Canonical runtime message model (`borg-agent`)
Files:
1. `crates/borg-agent/src/tools.rs`
2. `crates/borg-agent/src/message.rs`
3. `crates/borg-agent/src/agent.rs`
4. `crates/borg-agent/src/actor_thread.rs`
5. `crates/borg-agent/src/lib.rs`

Tasks:
1. Replace legacy tool-result enum usage with `ToolOutputEnvelope::{Ok,ByDesign,Error}`.
2. Ensure `Message::ToolResult` stores the canonical envelope.
3. Ensure `ToolCallRecord.output` stores the canonical envelope.
4. Keep `tool_call_id` pairing untouched and deterministic.

Exit criteria:
1. No runtime path depends on prose-oriented `ToolResultData` variants.

##Workstream 2: Adapter boundary and provider serialization
Files:
1. `crates/borg-agent/src/llm_adapter.rs`
2. `crates/borg-llm/src/types.rs`
3. `crates/borg-llm/src/providers/openai.rs`
4. `crates/borg-llm/src/providers/openrouter.rs`

Tasks:
1. Remove prose wrappers (`execution result in ...`, `tool error: ...`) from tool-result conversion.
2. Serialize canonical envelope as deterministic JSON payload.
3. Keep one fallback for providers that only accept text tool content:
   - emit minified JSON string of the same envelope.
4. Preserve orphan/interrupted-tool semantics with structured error envelopes.
5. Define deterministic serialization explicitly:
   - minified JSON,
   - stable serializer path for the same logical envelope.

Exit criteria:
1. No adapter path injects English wrappers as semantic tool-result content.
2. Same logical envelope serializes identically across runs.

##Workstream 3: Built-in tool output migration
Files:
1. `crates/borg-memory/src/tools.rs`
2. `crates/borg-taskgraph/src/tools.rs`
3. `crates/borg-schedule/src/tools.rs`
4. `crates/borg-fs/src/lib.rs`
5. `crates/borg-ports-tools/src/lib.rs`
6. `crates/borg-agent/src/admin_tools.rs`
7. `crates/borg-exec/src/tool_runner.rs`

Tasks:
1. Stop returning JSON-as-string for machine payloads.
2. Return structured payloads in `ToolOutputEnvelope::Ok`.
3. Use `ByDesign` when behavior is intentionally non-failing but non-mutating/no-op.
4. Reserve plain text output for genuinely human-textual tools only.

Exit criteria:
1. `rg "serde_json::to_string\\(.*\\)\\?\\)" crates/*/src/tools.rs crates/borg-exec/src/tool_runner.rs` no longer finds machine-payload stringification in tool returns.

##Workstream 4: Persistence and projections
Files:
1. `crates/borg-db/src/lib.rs`
2. `crates/borg-db/src/tools.rs` (if present in current branch split)
3. `crates/borg-gql/src/sdl/mod.rs`
4. `crates/borg-gql/src/sdl/resolvers/*.rs`
5. `crates/borg-gql/schema.graphql`

Tasks:
1. Ensure `tool_calls.output_json` stores canonical envelope data.
2. Ensure GraphQL projections expose structured envelope fields safely.
3. Remove assumptions that tool payloads are always text.

Exit criteria:
1. GraphQL/API surfaces can return structured tool outputs without lossy coercion.

##Workstream 5: Stage and DevMode rendering
Files:
1. `apps/stage/src/App.tsx`
2. `apps/devmode/src/**`

Tasks:
1. Render canonical envelope fields directly.
2. Keep call/result bundling by `tool_call_id`.
3. Default-collapse large tool payload blocks and allow expansion.

Exit criteria:
1. No `(non-text payload)` placeholder for structured tool results.
2. Tool call/result pairs render as one logical block.

##Workstream 6: Runtime consumers and CLI projections
Files:
1. `crates/borg-cli/src/cmd/tools/mod.rs`
2. `crates/borg-codemode/src/cli/mod.rs`
3. `crates/borg-shellmode/src/cli/mod.rs`
4. `crates/borg-memory/src/cli/mod.rs`
5. `crates/borg-taskgraph/src/cli/mod.rs`
6. `crates/borg-ports/src/telegram/mod.rs`
7. `crates/borg-exec/src/types.rs`

Tasks:
1. Migrate all consumers from `response.content` to `response.output`.
2. Remove assumptions about legacy `Text/Execution/Capabilities` variants.
3. Keep UX helpers that relied on duration by reading envelope payload fields (for example `duration_ms`).

Exit criteria:
1. Runtime and CLI projections compile and run without legacy variant matching.
2. Telegram/tool progress formatting remains functional after migration.

##Workstream 7: Destructive data reset
Mode:
1. destructive reset is acceptable for local/dev.

Tasks:
1. Clear legacy rows that depend on old text tool result shape.
2. Keep only canonical envelope payloads going forward.

Example cleanup (adjust tables if schema differs):

```sh
sqlite3 "$HOME/.borg/config.db" "BEGIN; DELETE FROM messages; DELETE FROM tool_calls; COMMIT;"
```

Exit criteria:
1. Runtime starts with clean rows for the new shape.

##Workstream 8: Tests
Files:
1. `crates/borg-agent/src/tests.rs`
2. `crates/borg-agent/tests/agentic_loop_with_real_llm.rs`
3. `crates/borg-memory/tests/rfd0005_property_suite.rs`
4. `crates/borg-memory/tests/rfd0005_test_suite.rs`
5. `crates/borg-taskgraph/src/tests.rs`

Tasks:
1. Replace assertions expecting prose tool-result text.
2. Add adapter tests for deterministic structured serialization.
3. Add interrupted/orphan tool-result regression tests for structured errors.

Exit criteria:
1. No tests depend on legacy prose wrappers.

##Validation gates
1. `cargo build`
2. `cargo test -p borg-agent -p borg-exec -p borg-gql -p borg-memory -p borg-taskgraph`
3. `bun run build:web`
4. Search gate:

```sh
rg "execution result in|tool error:|ToolResultData::Text|ToolResultData::Execution|ToolResultData::Capabilities" crates apps packages
```

Expected:
1. zero runtime hits for deprecated shape/wrappers,
2. remaining hits only in historical docs/fixtures explicitly accepted by reviewers.
3. no compile failures from `ToolResponse.content` / legacy variant usage in test targets.

##Execution order
1. Workstream 0
2. Workstream 1
3. Workstream 2
4. Workstream 3
5. Workstream 4
6. Workstream 5
7. Workstream 6
8. Workstream 7
9. Workstream 8
