# RFD0028 - Workspace Model Simplification

- Feature Name: `workspace_model_simplification`
- Start Date: `2026-03-05`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD proposes a large simplification pass over Borg's runtime and control-plane model.

Update (2026-03-07):
The identity model was simplified further after this draft and is now actor-only (`actor_id` only). See `RFD0030` for the final hard-cut semantics. Session-era wording below is historical context.

The target model is:

1. `Workspace`
2. `Actors`
3. `Ports`
4. `Providers`
5. `Projects`
6. `Tasks` (+ comments/events)
7. `Schedule` (renamed from Clockwork)

Primary cuts:

1. Remove `behavior + agent_spec` indirection from runtime execution paths.
2. Enforce actor-only runtime identity (`actor_id` only).
3. Remove legacy user-centric storage (`users`, `sessions.users_json`).
4. Unify duplicated port binding APIs into one binding model.
5. Remove legacy/compat storage paths (`port_session_ctx`, `message_index` sequencing).
6. Continue enforcing typed runtime contracts from `RFD0017`.
7. Remove the policy subsystem entirely (staged), including runtime/control-plane/API/storage surfaces.

This is a structural cleanup RFD. It intentionally prefers fewer concepts over compatibility shims.

## Status Snapshot (2026-03-06)

Completed in mainline rewrite work:

1. Legacy `@borg/api` frontend package removed from active app runtime paths.
2. Axum HTTP runtime surface moved under `borg-gql` (`/health`, `/ports/http`, plus `/gql*`).
3. Admin dashboard route graph hard-cut to active surfaces only (overview, providers, observability core).
4. Behavior UI removed from navigation/routes/pages.
5. Non-working legacy admin surfaces deleted instead of being kept behind compatibility placeholders.
6. Legacy `sessions`, `session_messages`, and `port_session_ctx` tables dropped; session state now derives from `port_bindings` + `messages`.
7. Schedule DB access now uses `sqlx::query!`/`query_as!` in active schedule paths (no dynamic `sqlx::query` needed there).
8. Port tool-action output formatting no longer depends on internal `serde_json::Value` plumbing.

Remaining high-priority cuts:

1. Keep enforcing typed-data boundaries so `serde_json::Value` stays restricted to explicit JSON boundary layers only (DB JSON columns, GraphQL `JsonValue`, provider/tool wire contracts).

Update (2026-03-07):
The actor-only hard cut from `RFD0030` is implemented in code. `sessions` and related runtime/API/storage surfaces were removed.

## Motivation
[motivation]: #motivation

Borg currently carries several overlapping abstractions that slow implementation and increase runtime complexity:

1. Actor execution requires resolving through multiple layers (`actor -> behavior -> agent_spec`) even though operations are actor/session-first.
2. A single actor currently carries multiple sessions, which complicates mailbox semantics and replay behavior.
3. Port binding logic is split into separate "session binding" and "actor binding" APIs over the same domain.
4. Session persistence still depends on user-era fields (`users_json`) and message indexing logic (`MAX(message_index)+1`).
5. Legacy compatibility code remains in active paths (`port_session_ctx` references).
6. Some subsystems are broader than needed for current v0 goals.

These create real costs:

1. More DB queries and more fallback branches per turn.
2. Harder mental model for contributors.
3. Higher risk of stale config and mismatched state.
4. Slower migration toward typed, compile-time checked internal data.

Borg already proved that simplification improves velocity (for example: typed agent/tool refactors and session-first runtime changes). This RFD formalizes the next simplification wave.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### New mental model

A Borg workspace should be understood as:

1. Actors do work.
2. Ports deliver messages into sessions and actors.
3. Providers power LLM/runtime capabilities.
4. Projects/tasks represent explicit planned work.
5. Schedule triggers future actions.

There is no separate behavior indirection required to run an actor.

Identity rule:

1. One actor owns one session.
2. One actor mailbox maps to that same session.
3. Delivery means the message is durably written to that session.
4. Ports that manage many external chats create one actor per chat and bind `(port, conversation_key)` to that actor.

### What contributors should expect

1. Runtime routing and execution decisions happen directly from actor + session state.
2. Port bindings are read/written through one API surface.
3. Session messages are ordered by durable IDs/timestamps, not mutable per-session indices.
4. User-era storage is removed from core runtime paths.
5. JSON remains boundary-only (per `RFD0017`), not an internal shared contract.

### Scope boundaries

In scope:

1. Data model cuts and endpoint consolidation.
2. Runtime simplifications that reduce per-turn resolution complexity.
3. Naming and subsystem alignment (`Clockwork` -> `Schedule`).
4. Planning full policy subsystem removal from core surfaces.

Out of scope:

1. Building new product features unrelated to simplification.
2. Changing the session-first ingress model.
3. Re-introducing compatibility layers that preserve old abstractions indefinitely.
4. Immediate replacement for all policy use-cases in this phase (removal is staged, not big-bang).

### Core vs profiles

Simplification constraint:

1. Borg core keeps only generic runtime primitives.
2. Use-case behavior is implemented as profile-level composition.

Core primitives are:

1. `Workspace`
2. `Actor` (`1 actor = 1 session = 1 mailbox`)
3. `PortBinding`
4. `Provider`
5. `Project`/`Task`
6. `Schedule`

Profile-level composition includes:

1. Bootstrap/setup defaults (`devmode`, `workspace swarm`, `personal assistant`)
2. Port-specific routing and context rules
3. Worker orchestration strategy (task pick/rebase/review)
4. Actor templates and tool grants for a specific workflow

Litmus test:

1. If a capability can be expressed with different actors, bindings, tools, or schedules, it is profile-level and should not become a new core subsystem.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## 1. Target domain model

`Workspace` owns these first-class entities:

1. `Actor`
2. `Port`
3. `Provider`
4. `Project`
5. `Task`
6. `ScheduleJob`

### 1.1 Actor contract

Actor should own execution fields directly:

1. model
2. provider preference/default
3. system prompt
4. tool grants
5. status/metadata

`behavior_id` and `agent_spec` are removed from runtime-critical paths.

### 1.2 Actor/session identity contract

Actor/session ownership is 1:1:

1. Each actor has exactly one primary session.
2. Each session has exactly one owning actor.
3. Actor mailbox identity is that same session identity.
4. Actor runtime does not multiplex independent conversations under one actor.

Operational effect:

1. Port ingress resolves `(port, conversation_key)` to actor id.
2. If no binding exists, the port may create a new actor + session pair and persist the binding.
3. Subsequent messages for that conversation go to the same actor/session pair.

### 1.3 Delivery and mailbox contract

Mailbox delivery boundary becomes unified message persistence:

1. A message is considered delivered when it is appended to the canonical `messages` store.
2. Queue processing and history are both represented from that same store (no separate mailbox table).
3. Cast/call transport semantics are expressed via typed payload/correlation metadata, not a table split.
4. `messages` rows are keyed by `message_id`, with `sender_id` + `receiver_id` as addressing fields.
5. There is no transition/message-kind filter at the storage layer.
6. The storage contract does not include a mailbox `kind` column or enqueue-time `kind` selector.

### 1.4 Port binding contract

Use one binding record keyed by `(port_uri, conversation_key)` with:

1. `session_id`
2. `actor_id`
3. optional transport context snapshot pointer/metadata

Remove separate actor-binding APIs and storage shims.

### 1.5 Session contract

Session record should not require user-era arrays.

1. Remove `sessions.users_json`.
2. Session message ordering uses monotonic identifiers and `created_at`.
3. Remove dependence on `message_index` for write/read/update/delete operations.

### 1.6 Schedule contract

`Clockwork` is renamed to `Schedule` at API/domain level. Internally, migration can be phased.

1. Entity names and API routes should move to `schedule_*` naming.
2. Existing scheduler semantics from `RFD0014` remain valid unless superseded.
3. Runtime should expose one schedule surface (DB/API/tools/CLI aligned).

### 1.7 Typed internal data

`RFD0017` remains mandatory:

1. No `serde_json::Value` in internal runtime contracts.
2. JSON only at explicit boundaries (API/provider/JS/DB codec).
3. DB read paths must parse to typed structs immediately.

## 2. Runtime simplification requirements

1. Remove fallback execution resolution branches that rely on legacy spec tables.
2. Enforce one actor/one session runtime ownership.
3. Ensure one clear source-of-truth for actor execution configuration.
4. Keep per-turn runtime path minimal: resolve binding -> load actor/session -> execute.

## 3. API simplification requirements

1. Remove behavior CRUD endpoints once actor fields are canonical.
2. Remove user CRUD endpoints once user-era storage is retired.
3. Consolidate port binding endpoints into one controller surface.
4. Keep control-plane CRUD/read surface in GraphQL (`/gql`, `/gql/ws`, `/gql/graphiql`).
5. Keep only runtime ingress REST endpoints outside GraphQL (`POST /ports/http`, `GET /health`).
6. Expose schedule endpoints under final naming in GraphQL (and keep runtime tools aligned).

## 4. Rewrite policy (no compatibility)

This RFD is a merciless rewrite and simplification effort.

1. No compatibility shims.
2. No backwards-compatible adapter layers.
3. No dual old/new API surfaces kept alive "for transition".
4. No requirement to preserve legacy wire formats or deprecated storage layouts.
5. Deprecated subsystems are removed, not wrapped.
6. If callers break, callers are rewritten to the new model.

Migration intent:

1. Keep durability and correctness where possible.
2. Prefer deleting legacy complexity over preserving old contracts.
3. Treat this as a restatement of the system model, not an incremental compatibility program.

## Implementation

This RFD executes in phases so each merge is small and testable.

### Implementation tracking (living checklist)

This section is the canonical execution tracker for this rewrite.

Rules:

1. Every simplification cut must be tracked here before/with code changes.
2. Do not rely on implicit knowledge or chat history for migration status.
3. Keep status values explicit: `done`, `in-progress`, `pending`.
4. This remains a merciless rewrite: no compatibility/back-compat tasks are tracked here.

#### Tracker (2026-03-06)

1. Dashboard route + nav contraction to active surfaces only.
   - Status: `done`
   - Scope:
   - `packages/borg-dashboard-control/src/app/routing.tsx`
   - `packages/borg-dashboard-control/src/app/navigation.ts`

2. Behavior UI removal (routes/pages/forms).
   - Status: `done`
   - Scope:
   - `packages/borg-dashboard-control/src/pages/control/behaviors/*`

3. Non-working admin surfaces hard-deleted (no placeholders).
   - Status: `done`
   - Scope:
   - `packages/borg-dashboard-control/src/pages/control/*` (legacy sections)
   - `packages/borg-dashboard-control/src/pages/{memory,taskgraph,fs}/*` (legacy sections)

4. Runtime actor execution no longer requires behavior lookup on hot path.
   - Status: `done`
   - Scope:
   - `crates/borg-exec/src/actor.rs`
   - `crates/borg-exec/src/session_manager.rs`

5. One actor instance enforces one owned session identity.
   - Status: `done`
   - Scope:
   - `crates/borg-exec/src/actor.rs`

6. Behavior GraphQL surface removal (`Behavior` object/query/mutation/node variant).
   - Status: `done`
   - Scope:
   - `crates/borg-gql/src/sdl/mod.rs`
   - `crates/borg-gql/src/sdl/resolvers/query.rs`
   - `crates/borg-gql/src/sdl/resolvers/mutation.rs`
   - `crates/borg-gql/src/tests.rs`

7. Behavior persistence/backend removal from active runtime path.
   - Status: `done`
   - Scope:
   - `crates/borg-db/src/behaviors.rs`
   - `crates/borg-db/src/lib.rs` (`BehaviorRecord`, module wiring)
   - `crates/borg-cli/src/app.rs` (planner behavior seeding)
   - New forward migration to drop behavior-era schema elements

8. Onboarding/devmode removal of behavior-era fields.
   - Status: `done`
   - Scope:
   - `packages/borg-onboarding/src/OnboardApp.tsx` (`behaviorId` flow)
   - `packages/borg-devmode/src/DevModeApp.tsx` (behavior discovery/selection)

9. GraphQL client artifact cleanup after behavior schema removal.
   - Status: `done`
   - Scope:
   - `packages/borg-graphql-client/src/generated/types.ts`
   - `packages/borg-graphql-client/src/generated/operations.ts`
   - `packages/borg-graphql-client/src/operations/*`

10. Policy subsystem full removal (runtime + schema + storage usage).
    - Status: `done`
    - Scope:
    - `crates/borg-db/src/policies.rs` and call sites
    - `crates/borg-gql/src/sdl/*` policy leftovers
    - Any remaining CLI/tool/runtime references

11. Schedule naming cut (`clockwork` -> `schedule`) across runtime/tooling/UI.
    - Status: `done`
    - Scope:
    - `crates/*clockwork*` and `Clockwork-*` tool names
    - GraphQL schema object/field naming
    - UI labels/routes/tool docs

12. Internal typed-data completion (RFD0017 alignment).
    - Status: `done`
    - Scope:
    - `serde_json::Value` remains allowed only at explicit JSON boundaries:
    - DB JSON codecs/columns (for example: `messages.payload_json`, `schedule_jobs.*_json`, `tool_calls.*_json`, `llm_calls.*_json`, port/app settings JSON)
    - GraphQL `JsonValue` scalar boundaries
    - Tool/provider wire-level JSON contracts
    - Internal domain/runtime flow should continue using typed structs/enums, with immediate parse at read boundaries.

13. Legacy session tables removal (`sessions`, `session_messages`, `port_session_ctx`) with bindings/messages-derived session state.
    - Status: `done`
    - Scope:
    - `crates/borg-db/migrations/0040_drop_sessions_tables.sql`
    - `crates/borg-db/src/sessions.rs` (session views derived from `port_bindings` + `messages`)
    - `crates/borg-db/src/ports.rs` (context/reasoning snapshot moved to `port_bindings`)
    - `crates/borg-ports/src/supervisor.rs` and `crates/borg-gql/src/lib.rs` (removed `ensure_session_row` dependency)

#### Done means

A tracker line is `done` only when all conditions hold:

1. Code path is removed/refactored in active runtime.
2. No stale route/schema/tool surface exposes the removed concept.
3. `cargo build` and `bun run build:web` pass.
4. Relevant tests for touched crates pass or are explicitly updated/removed.
5. This tracker line is updated in the same change window.

### Phase 1: Actor execution flattening

1. Add actor-owned execution fields (provider/model/system prompt/tool grants).
2. Backfill actor fields from current behavior/spec sources.
3. Update runtime to resolve directly from actor.
4. Remove behavior/spec reads from hot paths.

Exit criteria:

1. No runtime turn requires behavior/spec lookup.
2. Existing actor flows continue working.

### Phase 2: One actor = one session cutover

1. Add explicit actor ownership for session identity.
2. Stop multiplexing multiple independent sessions under a single actor runtime instance.
3. Route port conversations to actor/session pairs (create pair on first message when needed).
4. Move delivery boundary to "message appended to actor-owned session".

Exit criteria:

1. An actor processes exactly one session identity.
2. Port chats are isolated by actor/session pair.
3. Mailbox delivery is represented by durable session append semantics.

### Phase 3: Session/user cleanup

1. Remove `sessions.users_json` reads/writes from runtime.
2. Migrate session message APIs away from `message_index`.
3. Switch API handlers to ID/timestamp-based message addressing.
4. Drop `users` API dependencies from control-plane routes.
5. Introduce canonical `messages` table and remove `actor_mailbox` + `session_messages`.

Progress update (`2026-03-05`):

1. API ingress (`/ports/http`) and ports supervisor no longer read/merge `sessions.users_json` on hot path.
2. Session-row creation dependency was removed from hot paths; runtime no longer requires a dedicated `sessions` table row.
3. Canonical `messages` table was introduced with hard cutover: no backfill, and legacy `actor_mailbox`/`session_messages` tables are dropped.
4. Mailbox enqueue/read paths no longer expose or require a message-kind transition selector.
5. Session message DB operations no longer expose index-addressed get/update/delete methods; message-id addressing is canonical.
6. Legacy REST session-message handlers now use `message_id` path addressing for read/update/delete.
7. Port session-context and session reasoning-effort snapshots are stored directly on `port_bindings`.
8. `SessionRecord` and GraphQL `Session` surface no longer expose participant `users`; session identity is now session-id + port + timeline.
9. Session upsert APIs in DB/GraphQL no longer require a users list.
10. GraphQL `sessions(...)` no longer accepts/uses a `userId` filter; filtering is by port and pagination only.
11. GraphQL user root surfaces (`user`, `users`, `User` type) are removed from active schema/resolvers.
12. Runtime turn resolution no longer reads behavior records on the hot path; actor-owned prompt/model selection is used directly.
13. Runtime session state no longer carries behavior identity; `/model` reporting is actor/model/session only.
14. GraphQL policy root and node surfaces (`policy`, `policies`, `Policy`, `PolicyUse`, `Node::Policy`) are removed from active schema/resolvers.
15. Dashboard control navigation/route placeholders for policies were removed.
16. Active actor DB/GQL contracts no longer require or expose `default_behavior_id` / `defaultBehaviorId`.
17. `borg-exec` actor runtime now enforces one owned session id per actor instance and rejects cross-session delivery instead of multiplexing session state.
18. API client/UI contraction for actor behavior-era fields and REST route retirement was completed in the rewrite window tracked above.
19. Legacy `sessions`, `session_messages`, and `port_session_ctx` tables are dropped in forward migration; session listing/get now derives from `port_bindings` + `messages`.
20. Legacy `agent_specs` and `users` tables are dropped in forward migration; runtime no longer depends on those tables.
21. `taskgraph_tasks` identity columns are renamed to actor terminology (`assignee_actor_id`, `reviewer_actor_id`) and GraphQL/CLI payloads use `creatorActorId` / `assigneeActorId`.
22. Actor admin tools are actor-native (`Actors-*`) and no longer expose `Agents-*` naming.

Exit criteria:

1. Session writes no longer compute `MAX(message_index)+1`.
2. User tables are not required for runtime operation.

### Phase 4: Port binding consolidation

1. Unify actor/session binding code paths.
2. Remove duplicated binding modules/controllers.
3. Ensure actor-aware session resolution is single-path.

Exit criteria:

1. One binding CRUD/API surface exists.
2. Port ingestion uses one binding resolver.

### Phase 5: Schedule naming and surface alignment

1. Introduce `schedule` naming in API/tools/CLI.
2. Do not keep compatibility aliases; rewrite callers to `schedule` names.
3. Remove duplicate or conflicting scheduler surfaces.

Exit criteria:

1. Canonical route/tool/CLI names use `schedule`.
2. Legacy `clockwork` names no longer appear in runtime-facing API/tool/schema/UI surfaces.

### Phase 6: Typed-contract hardening and cleanup

1. Sweep remaining `serde_json::Value` internal contracts.
2. Replace with typed enums/structs per subsystem.
3. Remove transitional adapters introduced in earlier phases.

Exit criteria:

1. Internal hot paths satisfy `RFD0017`.
2. Typed boundaries are explicit and documented.

### Validation checklist (each phase)

1. `cargo build`
2. `cargo test` for touched crates
3. `bun run build:web` if API/contracts/UI are touched
4. Runtime smoke: `borg start` + port ingress + actor response

### Breaking change notes

Migration `0040_drop_sessions_tables.sql` is a hard cut with no compatibility or backfill:

1. Drops `sessions`, `session_messages`, `session_message_cursors`, and `port_session_ctx`.
2. Adds `context_snapshot_json` and `current_reasoning_effort` to `port_bindings`.
3. Session read/list semantics are derived from `port_bindings` + `messages`.
4. Existing local DBs with legacy table assumptions must be updated to the new model; old direct table queries will break.

### Detailed work map (codebase sweep)

This section maps concrete refactors needed in the current codebase.

Workstream 1: DB schema and typed records

1. Remove legacy model fields from DB records and APIs in `crates/borg-db/src/lib.rs`.
2. Replace `SessionRecord.users` and `session_messages.message_index` contracts in `crates/borg-db/src/sessions.rs`.
3. Remove `default_behavior_id` from actor records and writes in `crates/borg-db/src/actors.rs`.
4. Remove `schedule.message_type` and generic JSON payload shape in `crates/borg-db/src/schedule.rs`.
5. Add migrations for actor/session 1:1 ownership, message-id-based addressing, and legacy table retirement (`users`, behavior-era fields, queue-era fields).

Workstream 2: One actor = one session runtime cutover

1. Remove in-actor multi-session map (`HashMap<session_id, SessionState>`) in `crates/borg-exec/src/actor.rs`.
2. Replace execution config resolution through behavior/spec in `crates/borg-exec/src/actor.rs` and `crates/borg-exec/src/session_manager.rs`.
3. Remove `resolve_agent_id` fallback logic based on session event scan in `crates/borg-exec/src/session_manager.rs`.
4. Ensure actor runtime boot path assumes exactly one owned session and does not multiplex.

Workstream 3: Mailbox boundary simplification

1. Move durable delivery boundary to session append path in `crates/borg-exec/src/supervisor.rs`.
2. Retire or shrink envelope indirection in `crates/borg-exec/src/mailbox_envelope.rs`.
3. Replace `actor_mailbox` storage reads/writes with unified `messages` reads/writes.
4. Keep cast/call correlation metadata typed and envelope-level (`RFD0027`), not payload-level.

Workstream 4: Port binding unification and actor creation strategy

1. Merge session-binding and actor-binding paths in `crates/borg-db/src/ports.rs` and `crates/borg-db/src/actor_bindings.rs`.
2. Remove duplicated actor fallback logic and keep one canonical resolver path in `crates/borg-ports/src/supervisor.rs` + DB binding APIs.
3. On first `(port, conversation_key)` message, create actor+session pair if no binding exists.
4. Keep port-specific routing and context logic in `borg-ports`, not `borg-exec`.

Workstream 5: API route contraction

1. Remove behavior/user REST surfaces entirely and keep control-plane APIs GraphQL-first in `crates/borg-gql`.
2. Merge duplicated binding surfaces into one GraphQL binding model.
3. Keep only runtime ingress REST endpoints outside GraphQL (`/ports/http`, `/health`).
4. Rename GraphQL/tool-facing clockwork surfaces to schedule with no alias window.

Workstream 6: Actor CRUD and admin tooling

1. Remove `default_behavior_id` requirement from actor create/update in GraphQL actor mutations/resolvers.
2. Replace `Agents-*` admin tools with actor/runtime settings tools in `crates/borg-agent/src/admin_tools.rs` and `crates/borg-exec/src/tool_runner.rs`.
3. Remove `agent_specs` dependence from `crates/borg-agent/src/agent.rs` and `crates/borg-exec/src/session_manager.rs`.

Workstream 7: Ports supervisor and ingestion

1. Stop writing/merging session users lists in `crates/borg-ports/src/supervisor.rs`.
2. Keep context snapshot writes, but attach to canonical actor-owned session identity.
3. Ensure bridge loop resolves one binding path and one actor/session identity path.

Workstream 8: Schedule subsystem rename and narrowing

1. Rename clockwork CLI/API/tool names to schedule in `crates/borg-cli/src/cmd/schedule.rs`, `crates/borg-schedule/src/tools.rs`, and GraphQL schedule resolvers/schema.
2. Restrict scheduled payload shape to typed actor-deliverable message input.
3. Keep DB execution semantics from `RFD0014`, but align naming and typed contract.

Workstream 9: GraphQL surface alignment

1. Remove behavior and user schema objects/resolvers from `crates/borg-gql/src/sdl/mod.rs` and resolver modules.
2. Remove actor default-behavior fields and session users fields from GraphQL schema and resolvers.
3. Replace session message cursor/index API from `message_index` to stable message id.
4. Rename clockwork GraphQL objects to schedule equivalents.

Workstream 10: Frontend/API client contraction

1. Remove the deprecated `@borg/api` package entirely and keep frontend data access GraphQL-first via `@borg/graphql-client`.
2. Remove behavior/user routes/navigation/pages from `packages/borg-dashboard-control/src/app/routing.tsx`, `packages/borg-dashboard-control/src/app/navigation.ts`, and corresponding pages.
3. Update actor forms to stop requiring `defaultBehaviorId`.
4. Update sessions pages to remove `users` assumptions and message-index-based operations.
5. Rename clockwork UI and data hooks to schedule.
6. Hard-cut non-working admin surfaces: if a section does not have an active typed implementation, delete it and redirect routes instead of keeping placeholder or compatibility code.

Workstream 11: Taskgraph actor naming and scope trim

1. Rename `*_agent_id` fields to `*_actor_id` across `crates/borg-taskgraph/src/model.rs`, `store.rs`, `tools.rs`, `supervisor.rs`, CLI, and API bindings.
2. Keep only core task operations for v0 and defer advanced DAG operations.
3. Align status vocabulary with product lifecycle mapping.

Workstream 12: Typed-internal-data sweep completion

1. Remove remaining internal `serde_json::Value` hotspots identified in:
2. `crates/borg-agent/src/tools.rs`
3. `crates/borg-db/src/lib.rs`
4. `crates/borg-db/src/schedule.rs`
5. `crates/borg-db/src/ports.rs`
6. `crates/borg-db/src/llm_calls.rs`
7. `crates/borg-db/src/tool_calls.rs`
8. `crates/borg-ports/src/telegram/context_sync.rs`
9. `crates/borg-taskgraph/src/tools.rs`
10. Keep JSON only at explicit boundaries per `RFD0017`.

Workstream 13: Policy subsystem removal (staged)

1. Remove policy GraphQL query/mutation/object surfaces from `crates/borg-gql/src/sdl/*`.
2. Remove policy runtime/controller dependencies from active code paths.
3. Remove DB policy CRUD usage from active app/runtime flows.
4. Drop policy tables in a final migration after usage reaches zero.
5. Do not introduce replacement "policy subsystem" objects; keep constraints as typed actor/workspace config only when required.

Recommended execution order:

1. Workstreams 1, 2, 4
2. Workstreams 5, 6, 7
3. Workstreams 8, 9, 10
4. Workstreams 11, 12
5. Workstream 13

## Drawbacks
[drawbacks]: #drawbacks

1. This is a broad migration touching many crates.
2. Some existing scripts/integrations may break during endpoint renames.
3. Temporary migration adapters may add short-lived complexity.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Chosen approach: phased simplification with explicit cutovers.

Why this design:

1. It reduces concept count while keeping runtime behavior stable.
2. It aligns with already accepted direction (`RFD0017`, actor/session-first runtime).
3. It avoids indefinite compatibility debt.

Alternatives considered:

1. Keep current model and document it better.
   - Rejected: complexity is structural, not documentation-only.
2. Introduce additional adapters/facades without deleting old concepts.
   - Rejected: increases long-term maintenance burden.
3. Big-bang rewrite.
   - Rejected: high operational risk and hard rollback.

## Prior art
[prior-art]: #prior-art

Internal prior art:

1. `RFD0017` (typed internal contracts) established boundary discipline.
2. Session-first runtime changes reduced unnecessary task coupling.
3. Frontend workspace reset (`RFD0025`) showed that strategic simplification pays off.

General software architecture prior art:

1. Entity-model flattening to remove indirection layers in control planes.
2. Additive migration/cutover/drop pattern for safe schema evolution.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Final minimal taskgraph surface for v0 after simplification.
2. Whether to introduce a dedicated workspace settings table during this effort or in follow-up.

## Future possibilities
[future-possibilities]: #future-possibilities

1. First-class workspace defaults/templates (typed), applied to actors/ports.
2. Schedule integration with task/project workflows (recurring planning/review automation).
3. Multi-workspace support once single-workspace model is fully simplified.
4. Further API regrouping under workspace-scoped routes after legacy endpoint retirement.
5. Add typed workspace-level constraints only if strictly needed, without reintroducing a policy subsystem.
