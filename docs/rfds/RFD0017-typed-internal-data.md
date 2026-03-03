# RFD0017 - Typed Internal Data (No JSON in Runtime)

- Feature Name: `typed_internal_data_contract`
- Start Date: `2026-03-03`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD defines a hard runtime contract for Borg:

1. Borg internal code must use typed Rust data structures only.
2. `serde_json::Value` is forbidden in internal runtime/domain contracts.
3. JSON is allowed only at explicit external boundaries (API/provider/JS FFI/DB codec).
4. Boundary JSON must be parsed into typed structs immediately.
5. Database JSON columns may remain for compatibility, but `borg-db` APIs must return typed structs immediately after read.

This document includes a sweep of current JSON usage and a phased refactor plan.

## Motivation
[motivation]: #motivation

Current runtime behavior allows untyped JSON values to flow across core paths (ports -> exec -> mailbox -> agent -> db). That creates three problems:

1. Contract drift: message shape is implicit and changes silently.
2. Runtime fragility: invalid or unexpected keys fail late.
3. Ownership blur: crates depend on JSON shape knowledge they should not own.

The immediate symptom is confusion around message semantics (`BorgInput::Chat` vs generic payload/message_type), plus transport-specific logic leaking into runtime internals.

A typed-only internal contract fixes this by making all message/context/schedule/state shapes explicit and compile-time checked.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Non-negotiable rules

1. No `serde_json::Value` in internal runtime structs, enums, or method signatures.
2. If a port receives JSON from an external API, it parses to typed structs in that port module immediately.
3. If a provider needs JSON over HTTP, provider code serializes typed structs only at send time.
4. If DB stores JSON text, `borg-db` parses it into typed records before returning values.
5. Internal actor delivery is typed and constrained to supported message variants.

### Boundary definition

JSON is allowed only in these places:

1. HTTP request/response parsing at API edges.
2. Third-party SDK/API payload boundaries (Telegram/Discord/OpenAI/etc.).
3. CodeMode JS FFI boundary.
4. DB serialization/deserialization codec layer.

JSON is not allowed across crate boundaries inside runtime logic.

### Clockwork implication

Clockwork must schedule only deliverable typed actor input. For v0 this means:

1. Deliverable input is `borg_exec::message::BorgInput::Chat { text }`.
2. `message_type` is not user-editable and should not be persisted as arbitrary input.
3. Clockwork create/update payload is typed (chat text + typed schedule), not arbitrary JSON.

### Context architecture implication

`borg-agent` must not know about specific ports such as Telegram. Context data is sourced through providers.

1. `ContextManager` is a regular struct (not a trait hierarchy).
2. `ContextManager` uses a configured policy enum (`Passthrough` or `Compacting`).
3. Providers implement a typed trait and return context chunks with explicit compaction mode.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMode {
    Pinned,
    Compactable,
}

#[derive(Debug, Clone)]
pub struct ContextChunk {
    pub mode: ContextMode,
    pub messages: Vec<Message>,
}

#[async_trait::async_trait]
pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn get_context(&self) -> anyhow::Result<Vec<ContextChunk>>;
}
```

This allows one provider to return both pinned and compactable chunks.

### Tooling architecture implication

Tool calls/responses are typed runtime contracts. JSON for tool arguments exists only at provider boundary.

1. The provider returns tool-call blocks with JSON arguments.
2. `borg-agent` decodes those arguments into typed tool requests before dispatch.
3. Tool dispatch runs on typed request/response enums.
4. Message/session storage persists typed message envelopes (serde), not `Value` blobs.
5. Runtime execution no longer depends on ad-hoc JSON-schema validation logic.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## Sweep findings (current state)

Sweep result at time of writing:

1. 41 files import `serde_json::Value` in `crates/*`.
2. Highest concentration: `borg-db`, `borg-exec`, `borg-ports`, `borg-agent`.

High-impact internal JSON contracts discovered:

1. `crates/borg-exec/src/types.rs`
   - `UserMessage.metadata: Value`
   - `ToolCallSummary.arguments/output: Value`
2. `crates/borg-exec/src/port_context.rs`
   - `PortContext` trait merges/exports `Value`.
3. `crates/borg-exec/src/mailbox_envelope.rs`
   - mailbox payload includes `port_context: Value`.
4. `crates/borg-agent/src/tools.rs`
   - tool request/spec/validation contract is `Value`-based.
5. `crates/borg-db/src/sessions.rs`
   - session message APIs accept/return `Value`.
6. `crates/borg-db/src/actors.rs`
   - actor mailbox payload is parsed as `Value`.
7. `crates/borg-db/src/clockwork.rs`
   - job payload/headers/schedule are `Value`; includes mutable `message_type`.
8. `crates/borg-ports/src/telegram/context_sync.rs` and `crates/borg-ports/src/discord/mod.rs`
   - session context is built/merged as untyped JSON.

These are internal runtime paths, not just external boundaries.

## Target typed model

### 1. Message path types

Introduce typed domain objects and remove untyped maps from runtime:

1. `InboundPortMessage` with typed `PortMessageContext` enum.
2. `ActorMailboxEnvelope` carrying typed context enum.
3. `SessionMessage` persistence API using typed `borg_agent::Message`.
4. `ToolCallSummary` with typed arguments/results structs (or typed enums per tool system), not raw `Value`.

### 1.1 Typed message generics for tools

Message contracts are parameterized by tool call/response types.

```rust
pub enum Message<TToolCall, TToolResponse> {
    System { content: String },
    User { content: String },
    Assistant { content: String },
    ToolCall { tool_call_id: String, call: TToolCall },
    ToolResult { tool_call_id: String, response: TToolResponse },
    SessionEvent { name: String, payload: SessionEventPayload },
}
```

Requirements:

1. `TToolCall: Serialize + Deserialize`.
2. `TToolResponse: Serialize + Deserialize`.
3. Runtime concrete alias uses Borg-wide enums (`BorgToolRequest`, `BorgToolResponse`).

This keeps DB persistence typed via serde while avoiding runtime `Value`.

### 2. Port context ownership

Move all transport parsing/merging to `borg-ports`:

1. `telegram` module owns Telegram context parsing + merge.
2. `discord` module owns Discord context parsing + merge.
3. `borg-exec` receives already-typed context and does not special-case by port name.

### 2.1 Context orchestration contract

Replace specialized context-manager variants with one configured manager and provider composition.

1. `ContextManager` becomes a concrete struct.
2. `ContextManager` policy is data-driven (`Passthrough` or `Compacting { ... }`).
3. Providers are injected through builder-style registration.
4. Provider output is `Vec<ContextChunk>`, so each provider may contribute mixed `Pinned` and `Compactable` segments.

Expected builder shape:

```rust
let manager = ContextManager::builder()
    .with_policy(ContextPolicy::Compacting { /* ... */ })
    .add_provider(telegram_context_provider)
    .add_provider(actor_context_provider)
    .add_provider(tools_context_provider)
    .build()?;
```

Deterministic ordering rules:

1. Providers are processed in registration order.
2. Chunks keep provider-return order.
3. `Pinned` chunks are never compacted.
4. Compaction only applies to `Compactable` chunks.

### 3. Clockwork typing

Clockwork job model should be:

1. `ClockworkTarget { actor_id, session_id }`
2. `ClockworkPayload::Chat { text: String }`
3. `ClockworkSchedule::{ Once { run_at }, Cron { expr } }`
4. `ClockworkStatus::{ Active, Paused, Cancelled, Completed }`

No free-form payload, no editable message type.

### 4. DB API contract

`borg-db` methods must expose typed records only.

1. Parse at read boundary: DB row -> typed struct.
2. Serialize at write boundary: typed struct -> persisted column.
3. Do not expose `Value` in public `borg-db` record structs for runtime paths.

### 5. Typed tool dispatch contract

Tool dispatch is strongly typed in runtime, dynamic only at boundary lookup.

1. Use Borg-wide typed request/response enums:
   - `BorgToolRequest`
   - `BorgToolResponse`
2. `Toolchain` is a regular struct dispatching typed requests.
3. Dynamic runtime enable/disable is maintained by tool-name gating before dispatch.
4. Provider boundary adapter maps:
   - `(tool_name, arguments_json)` -> `BorgToolRequest`
   - `BorgToolResponse` -> provider/output JSON when needed

`ToolRunner`/`AgentTools` indirection is not required as a core runtime contract and can be collapsed into a direct typed toolchain API.

### 5.1 Tool result typing

`ToolResultData` becomes fully typed by replacing execution payload `Value`.

Target shape:

```rust
pub enum ToolResultData<TToolResponse> {
    Execution { result: TToolResponse, duration: Duration },
    Error { message: String },
}
```

No `Execution.result: Value` in runtime contracts.

### 5.2 Validation model

Input/output correctness is defined by serde decode/encode of typed request/response types.

1. Input validation: JSON boundary decode into typed request.
2. Output validation: typed response must serialize successfully.
3. Ad-hoc runtime JSON-schema validation (`validate_schema`) is removed from execution path.
4. Schema generation/introspection can be derived from types as a separate concern.

## Implementation
[implementation]: #implementation

### Phase 0 - Guardrails

1. Add lint/policy checks to block new `serde_json::Value` in runtime/domain crates.
2. Allow temporary exceptions only in boundary modules (`api`, provider adapters, codemode ffi, db codec internals).

### Phase 1 - Port typing first

1. Define typed inbound metadata structs per port.
2. Replace JSON context merge in `borg-ports` with typed merge functions.
3. Persist typed session context snapshots through typed db methods.

### Phase 2 - Exec/agent message path

1. Replace `UserMessage.metadata: Value` with typed metadata/context.
2. Replace `PortContext` JSON trait methods with typed trait/enum methods.
3. Replace mailbox envelope JSON context field with typed context enum.
4. Replace `PassthroughContextManager` / `CompactingContextManager` split with one `ContextManager` struct and policy enum.
5. Introduce `ContextProvider` + `ContextChunk` contract in `borg-agent`; move Telegram-specific context provisioning out of `borg-agent`.
6. Parameterize `Message` by typed tool request/response and adopt Borg runtime aliases.

#### Progress notes (2026-03-03)

1. `crates/borg-exec/src/port_context.rs` now uses a typed `PortContext` enum (`Telegram|Discord|Http|Unknown`) with typed per-port structs; JSON trait-object context methods were removed.
2. `BorgMessage.port_context` and `PortMessage.port_context` now carry typed `PortContext` instead of `Arc<dyn PortContext>`.
3. `ActorMailboxEnvelope.port_context` now carries typed `PortContext` (no internal `serde_json::Value` field), serialized/deserialized only at DB boundary with serde.
4. Runtime/port call sites were updated to use enum accessors (`as_telegram`/`as_discord`) instead of downcasting JSON wrappers.
5. `cargo test -p borg-exec` passes with these changes.
6. API-internal validated port ingress DTOs in `borg-api` no longer carry passthrough metadata JSON after boundary validation (`ValidatedPortRequest` / `ValidatedHttpPortAudioRequest` dropped `metadata: Value`).
7. Runtime tool-call/result contracts in `borg-agent` now use concrete `BorgToolCall` / `BorgToolResult` types end-to-end in `borg-exec` + `borg-ports` message flow, replacing direct `Value` generic usage in these internal signatures.
8. Legacy tool implementations still parse JSON arguments in tool modules, but adaptation happens in `Tool::new` (`BorgToolCall` -> parsed JSON for callback, callback JSON result -> `BorgToolResult`) so exec/ports/session path signatures no longer use `Value` directly.
9. `borg-agent::Agent::run` no longer stores intermediate tool-call arguments as `Value`; tool blocks are decoded directly into typed `TToolCall` before dispatch.
10. Added `Tool::new_transcoded` adapter in `borg-agent` so tool modules can declare typed request/response structs while still registering into the unified runtime toolchain.
11. Migrated `borg-agent` admin tools (`Agents-*`) to typed request DTOs (`ListAgentsArgs`, `CreateAgentArgs`, etc.) and removed `Value`-shaped request parsing from that module’s execution path.
12. Migrated `borg-ports-tools` (`Ports-*`) to typed request DTOs with `Tool::new_transcoded`; request parsing now happens via serde decode into typed structs instead of ad-hoc `Value` field lookups.
13. Migrated `borg-shellmode` command tool request parsing to a typed DTO (`ExecuteCommandArgs`) via `Tool::new_transcoded`.
14. Migrated `borg-codemode` tool request parsing (`CodeMode-searchApis` and `CodeMode-executeCode`) to typed DTOs (`SearchApisArgs`, `ExecuteCodeArgs`) via `Tool::new_transcoded`.
15. Migrated `borg-clockwork` tools (`Clockwork-*`) to typed request DTOs (`CreateJobArgs`, `UpdateJobArgs`, etc.) with explicit typed schedule input enum, removing ad-hoc request `Value` field access in clockwork tool execution.
16. Updated Telegram output formatting in `borg-ports` to consume typed runtime tool-call wrappers directly (`RuntimeToolCall`), decoding at the formatting boundary instead of depending on raw `serde_json::Value` argument types.
17. Migrated `borg-apps` built-in discovery tools (`Apps-listApps`, `Apps-getApp`) to typed request DTOs and removed request argument map access via `Value` in the execution path.
18. Migrated `borg-fs` tool request handling to typed DTO (`FsToolArgs`) and removed ad-hoc `Value` map parsing helpers (`read_string` / `read_bool` / `read_u64`) from the runtime tool execution path.
19. Began `borg-taskgraph` migration by converting core tool request parsing for `TaskGraph-createTask`, `TaskGraph-getTask`, and `TaskGraph-updateTaskFields` to typed DTOs (`CreateTaskArgs`, `GetTaskArgs`, `UpdateTaskFieldsArgs`).

### Phase 3 - `borg-db` typed APIs

1. Change session/actor mailbox methods to typed payload params and returns.
2. Keep existing JSON columns initially for compatibility.
3. Centralize parse/serialize only inside db codec helpers.
4. Ensure session/tool-call records deserialize into typed request/response envelopes at read time.

### Phase 4 - Clockwork hardening

1. Remove mutable `message_type` from create/update surfaces.
2. Restrict job payload to chat text typed field.
3. Replace `schedule_spec: Value` with typed schedule enum in runtime/db APIs.
4. Update CLI/UI forms to only collect supported typed fields.

### Phase 5 - Tooling runtime hardening

1. Replace `ToolRequest.arguments: Value` with typed Borg request envelope.
2. Replace `Message::ToolCall.arguments: Value` and summary/output `Value` fields with typed envelopes.
3. Replace `ToolResultData::Execution.result: Value` with typed response.
4. Remove runtime `validate_schema` execution-path dependency in `borg-agent`.
5. Keep JSON only in provider/API/FFI/DB codec adapters.

### Phase 6 - Cleanup and enforcement

1. Remove remaining internal `Value` in runtime crates.
2. Keep `Value` only in strict boundary modules.
3. Add tests that fail if internal runtime types regress to untyped JSON.

### Current implementation findings (2026-03-03)

Implemented in this branch so far:

1. Collapsed tool execution indirection in runtime path:
   - removed `ToolRunner` trait and `AgentTools` wrapper from `borg-agent`
   - `Agent::run` now receives `&Toolchain` directly and dispatches through `Toolchain::run(ToolRequest { ... })`
   - removed `call_tool` helper from runtime path; tests now also execute tools via `Toolchain::run`
2. Updated downstream call sites/tests to use direct `Toolchain` execution:
   - `borg-exec` actor path and crate tests
   - `borg-agent` unit/integration tests
   - CLI/test imports in `borg-memory`, `borg-taskgraph`, `borg-shellmode`, `borg-codemode`, and `borg-cli`
3. Removed ad-hoc schema gate from agent execution path:
   - `Toolchain::run` no longer calls runtime `validate_schema` on input/output
   - validation responsibility moves to typed decode/encode at boundaries and tool callback logic
4. Context orchestration refactor started:
   - replaced trait-based manager variants with a concrete `ContextManager` configured by `ContextManagerStrategy`
   - introduced `ContextProvider::get_context() -> Result<Vec<ContextChunk>>` where each `ContextChunk` is `Pinned` or `Compactable`
   - removed Telegram-specific context logic from `borg-agent`; port-specific pinned context is now injected from `borg-exec` via `StaticContextProvider`
   - removed duplicate Telegram context re-injection in `borg-exec::actor` (session manager now owns this context wiring)
   - added coverage for `Pinned` provider chunks surviving compaction in `borg-agent` tests
5. Typed tool output wiring started in exec:
   - `borg-exec` tool-call summaries now carry `ToolResultData` directly (instead of converting output to `serde_json::Value` in actor runtime)
   - `ToolCallSummary::error_message` / `output_message` now match on typed tool result variants
6. Removed `UserMessage` intermediate ingress DTO from active exec path:
   - session/toolchain wiring now uses explicit typed fields instead of passing a generic ingress object through runtime internals
7. Removed `UserMessageMetadata` from exec runtime wiring:
   - `borg-exec` no longer exports/depends on a `UserMessageMetadata` ingress struct
   - API `ValidatedPortRequest.metadata` is currently passed through as boundary JSON into `JsonPortContext`
   - runtime toolchain build path now takes explicit typed identifiers only (`user_id`, `session_id`, `agent_id`)
8. Tool runtime foundations are now typed-generic:
   - `Toolchain<TToolCall, TToolResult>` and `Tool<TToolCall, TToolResult>` now support typed execution envelopes (with `Value` defaults preserved)
   - added `Tool::new_typed(...)` while keeping `Tool::new(...)` for current JSON-first call sites to avoid broad inference churn
9. Session/context pipeline now carries tool generics:
   - `Session<TToolCall, TToolResult>`, `ContextWindow<TToolCall, TToolResult>`, `ContextChunk<TToolCall, TToolResult>`, and `ContextManager<TToolCall, TToolResult>` are generic with `Value` defaults
   - `Agent::run(...)` is now generic and decodes provider tool-call JSON into `TToolCall` at the provider boundary
   - LLM adapter now serializes typed tool-call arguments/results back to provider-facing JSON/text at the boundary
10. Added regression coverage for typed dispatch:
   - new `borg-agent` test exercises a typed toolchain (`EchoArgs`/`EchoResult`) and verifies provider JSON arguments decode into strongly typed tool requests before tool execution
11. Extended typed envelopes one layer up into `borg-exec`:
   - `ToolCallSummary<TToolCall, TToolResult>` and `SessionOutput<TToolCall, TToolResult>` now accept typed arguments/results with `Value` defaults
   - this keeps current behavior intact while allowing typed session output/tool summaries to propagate as downstream crates adopt concrete tool enums
12. Removed implicit JSON generic defaults from core runtime contracts:
   - `Agent`, `Session`, `Message`, `Context*`, `Tool*`, and `Toolchain` no longer default type parameters to `serde_json::Value`
   - all call sites now specify concrete tool call/result types explicitly, eliminating hidden JSON fallback paths in signatures
13. Extended the same explicit-typing rule to `borg-exec` output contracts:
   - `SessionOutput<TToolCall, TToolResult>` and `ToolCallSummary<TToolCall, TToolResult>` now require explicit type parameters at all call sites
   - no implicit `Value` default exists in `borg-exec` runtime-facing message/output types
14. Migrated the remaining `borg-taskgraph` tool handlers away from ad-hoc `Value` parsing:
   - converted all `TaskGraph-*` handlers to `Tool::new_transcoded(...)` with typed request DTOs
   - removed `req_str`/`str_array` JSON walkers from taskgraph tool execution paths
   - pagination and status/default handling now come from typed request structs plus small normalization helpers
15. Started migrating `borg-memory` tool ingress to typed DTOs:
   - converted `Memory-newEntity`, `Memory-saveFacts`, and `Memory-searchMemory` to `Tool::new_transcoded(...)`
   - replaced local `serde_json::from_value` argument decoding in those handlers with typed request structs
   - kept wire/output format unchanged (`ToolResultData::Text`) to avoid behavior drift while continuing the migration
16. Expanded `borg-memory` typed ingress coverage for schema tools:
   - converted `Memory-Schema-defineNamespace`, `Memory-Schema-defineKind`, and `Memory-Schema-defineField` to typed DTO requests
   - removed per-field `Value` probing (`get(...).and_then(...)`) from those handlers
   - retained existing validation messages and output payload shape while moving argument parsing to serde
17. Migrated additional `borg-memory` read/search/entity handlers to typed DTOs:
   - converted `Memory-search`, `Memory-createEntity`, `Memory-getEntity`, and `Memory-listFacts` to `Tool::new_transcoded(...)`
   - removed JSON map-walking in those handlers and moved option/default handling into typed request models
   - preserved all current output payloads and semantic behavior while changing only argument decoding
18. Migrated `borg-memory` write/retract ingress wrappers to typed DTOs:
   - converted `Memory-stateFacts` and `Memory-retractFacts` handlers to typed request structs
   - removed ad-hoc object traversal for required fields in those handlers
   - retained existing polymorphic value parsing (`parse_rfd_value`) and output shape while tightening top-level request typing
19. Completed `borg-memory` tool constructor migration to typed adapters:
   - switched the remaining `Memory-getSchema` handler to `Tool::new_transcoded(...)` with an explicit empty request DTO
   - `borg-memory/src/tools.rs` production handlers now consistently use typed request decoding through `new_transcoded`

Important behavior change from these updates:

1. `ToolSpec.parameters` and `Tool.output_schema` are no longer enforced by `borg-agent` at runtime.
2. Existing tests that asserted schema-rejection behavior were updated to reflect callback-owned validation.
3. `ContextManager` now compacts only chunks marked `Compactable`; `Pinned` chunks are never compacted.
4. Exec-level tool summaries no longer rely on JSON object-shape probing for errors.
5. Port metadata still enters through `JsonPortContext`; removing that boundary JSON adapter remains pending.
6. Some built-in tools still use `Tool::new(...)` with JSON arguments; taskgraph and memory production tool handlers are now fully migrated to typed ingress decoding, with remaining runtime dynamic adapters concentrated elsewhere (for example provider-admin bridging in exec).

Known blocker while validating this branch:

1. Workspace checks can be blocked by unrelated in-flight `borg-db` compile issues from parallel work (outside this RFD track). This does not change the typed-agent design direction, but it can temporarily reduce end-to-end check coverage on this branch.

Immediate follow-up work after this pass:

1. Replace string-prefixed Telegram context system messages with typed provider payload structs.
2. Remove remaining `serde_json::Value` usage in exec/session-facing types (`ToolCallSummary.arguments`, legacy JSON port context adapters).
3. Type `ToolRequest.arguments` and `Message::ToolCall.arguments` with Borg request enums at agent dispatch boundary.
4. Replace API `ValidatedPortRequest` and remaining port ingress wrappers with a single typed ingress schema shared with port supervisors.

### Test plan

1. Unit tests per boundary parser (JSON -> typed).
2. Roundtrip db codec tests (typed -> JSON column -> typed).
3. End-to-end session turn tests validating typed flow from port ingress to mailbox delivery.
4. Clockwork tests proving only `BorgInput::Chat { text }` is schedulable.
5. Tool-dispatch tests validating `(tool_name, arguments_json)` decodes to typed request and returns typed response.

## Drawbacks
[drawbacks]: #drawbacks

1. Large refactor touching many crates and signatures.
2. Transitional adapters will temporarily increase code volume.
3. Some highly dynamic domains (tool schemas/JS FFI) need explicit typed wrappers/enums, which adds design work.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Chosen approach: strict typed internals with explicit boundary codecs.

Alternatives considered:

1. Keep `Value` internally and rely on validation.
   - Rejected: still permits silent shape drift and late failures.
2. Keep mixed model (typed in some places, JSON in others).
   - Rejected: does not solve ownership leakage and repeated parsing logic.
3. Migrate DB schemas to fully normalized typed columns immediately.
   - Deferred: larger migration risk; codec-first keeps rollout incremental.

## Prior art
[prior-art]: #prior-art

1. “Parse at the edges” architecture in strongly typed service codebases.
2. ADT-based message buses replacing free-form JSON payloads.
3. Event-sourced systems that keep JSON storage but typed domain APIs.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. For app auth config, do we use a closed enum (`none|oauth2|api_key`) now or keep an extension mechanism with typed plugin config codecs?
2. Should provider failure handling be configurable per provider (`required` vs best-effort), or global at manager level?
3. For tool schema publication, do we derive schemas from types directly now, or keep transitional manually-authored schema metadata?

## Future possibilities
[future-possibilities]: #future-possibilities

1. A dedicated `borg-types` crate for shared typed envelopes used by ports/exec/db.
2. Compile-time checks that ban `serde_json::Value` outside allowlisted boundary modules.
3. Optional binary codecs (for example, postcard) for internal persistence after JSON-free domain migration is complete.
