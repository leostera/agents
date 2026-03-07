# RFD0030 - Actor-Only Runtime (No Sessions)

##Status
Implemented

##Summary
Borg removes the session concept completely from runtime, API, storage, and UI.
There is exactly one routing identity: `actor_id`.

If a message is sent, it is sent to an actor.
If context is persisted, it is persisted for an actor.
If a port maps an external conversation, it maps directly to an actor.

This is a hard cut with no backward-compatibility layer.

##Motivation
Current code still contains `session_id` in:
- DB schemas and query APIs (`messages`, `port_bindings`, `tool_calls`, schedule rows).
- Runtime structs (`BorgMessage`, `SessionOutput`, mailbox records).
- GraphQL schema/types/queries (`Session*`, `sessionId`, session subscriptions/mutations).
- UI and SDK payloads (Stage app actor mailbox views, onboarding/devmode flows).

This creates dual identity models (`actor_id` + `session_id`) and causes runtime confusion.

##Decisions
1. Identity model:
`actor_id` is the only runtime identity. `session_id`/`session_uri` are removed.

2. Port routing:
`port + conversation_key -> actor_id` only.
No session binding row and no session fallback behavior.

3. Message storage:
`messages` rows are actor-addressed (`sender_id`, `receiver_id`) without `session_id`.

4. Context storage:
Port context snapshots and reasoning effort are keyed by actor, not session.

5. Taskgraph and schedule:
Any assignee/reviewer/target session fields become actor fields.

6. API:
GraphQL and HTTP ingress remove session inputs/outputs.

7. Compatibility:
No transition shim. Session APIs and fields are deleted.

##Non-goals
1. Migrating old data to preserve session identifiers.
2. Supporting mixed actor/session operation.
3. Keeping old GraphQL fields as aliases.

##Implementation
###1. Storage + DB API
1. Add a new migration that:
- drops `messages.session_id`,
- drops `port_bindings.session_id`,
- removes schedule `target_session_id`,
- removes session-oriented helper tables/indices if still present.
2. Remove `sessions.rs` from `borg-db`.
3. Delete `SessionRecord` / `SessionMessageRecord` from `borg-db` public types.
4. Replace session-oriented DB methods with actor-oriented ones:
- message history by `actor_id`,
- context snapshot by `actor_id`,
- reasoning effort by `actor_id`.

###2. Runtime (`borg-exec` + `borg-ports`)
1. Remove `session_id` from `BorgMessage` and runtime outputs.
2. Remove session-specific checks and canonicalization logic.
3. Replace `SessionManager` usage with actor-only context/history manager semantics.
4. Mailbox enqueue/claim paths operate on actor ids only.
5. Port supervisor stops resolving sessions; resolves actor binding directly.

###3. GraphQL (`borg-gql`)
1. Remove `Session*` object graph and session subscriptions.
2. Remove session mutations/inputs and `sessionId` fields.
3. Actor object exposes actor message history directly.
4. Port binding object exposes actor routing only.
5. HTTP ingress endpoint removes `session_id` and no longer emits `x-borg-session-id`.

###4. Apps/UI/SDK
1. Stage UI queries/mutations move from `actor.sessions.messages` to actor-only messages.
2. SDK operation types remove `sessionId` fields where still present.
3. Devmode/onboarding references to fixed planning sessions are replaced with actor ids.

###5. Taskgraph/Schedule
1. Rename remaining session-assignee/reviewer/target fields to actor equivalents.
2. Dispatch and supervisor loops key off actor ids only.

###6. Build + verification
1. `cargo build`
2. `cargo test` for touched crates
3. `bun run build:web`
4. Search gate:
`rg "session_id|session_uri|\\bSession\\b" crates packages apps --glob '!crates/borg-db/migrations/**'`
must only return intentionally retained third-party/storybook text or explicitly accepted exceptions.

##Implementation Status (Current Branch)
Completed:
1. `borg-db` hard cut:
- deleted `sessions.rs`,
- removed session DB records/types,
- actor-only message/context/reasoning APIs,
- actor-only schedule model (no `target_session_id`),
- actor-only tool-call persistence.
2. Added destructive migration:
- `0045_actor_only_runtime_hard_cut.sql` (drops/recreates actor-only runtime tables).
3. Added destructive TaskGraph hard-cut migration:
- `0046_taskgraph_actor_only_hard_cut.sql` (drops/recreates taskgraph tables with actor-only identity columns).
4. `borg-exec` actor-only runtime wiring:
- `BorgMessage` has no `session_id`,
- output type is actor-oriented,
- mailbox envelope/message flow actor-only.
5. `borg-gql` hard cut:
- removed `Session*` GraphQL surfaces,
- actor history exposed through actor message types,
- HTTP ingress uses actor routing and emits actor id header.
6. `borg-taskgraph` actor-auth terminology:
- removed session fields from Rust task model/event projections,
- tool arguments switched to `actor_id`,
- store auth/error terminology moved to actor-based semantics.
7. `borg-fs` tool/input metadata switched from `session_id` to `actor_id`.
8. `borg-cli` actor terminology update:
- actor stream/clear history paths,
- actor maintenance admin command paths.
9. Source-level frontend/client cleanup:
- `borg-ui` Session component/type renamed to actor timeline/message naming,
- onboarding GraphQL client logic switched to actor message queries,
- stage/devmode source no longer filters or emits `session_event`.
10. Legacy session-era crate test modules removed/replaced where incompatible with the hard cut:
- removed stale `borg-exec` and `borg-gql` session-era test modules,
- updated `borg-agent` and `borg-taskgraph` test suites to actor-only semantics.
11. GraphQL client artifacts and Storybook/demo text normalized to actor-only naming.

Current verification:
1. `cargo build` passes.
2. `cargo test` passes.
3. `bun run build:web` passes.
4. Source gate passes:
`rg "session_id|session_uri|\\bSession\\b|\\bsession\\b|session_|_session|session_event" crates packages apps --glob '!crates/borg-db/migrations/**' --glob '!docs/**'`

Remaining cleanup:
1. Legacy narrative RFDs still contain historical session-era wording and are intentionally left as historical records.

##Risks
1. Broad API breakage for clients expecting session identifiers.
2. Migration failures on older local DBs with divergent schema state.
3. Large refactor touching runtime, DB, and GraphQL simultaneously.

##Rollout
Single hard-cut rollout on `main`.
No compatibility mode.
