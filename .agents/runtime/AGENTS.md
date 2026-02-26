# Runtime Agent Guide

Scope: Rust runtime behavior, session turns, explicit tasks, storage wiring, and server lifecycle.

## Current Runtime Contracts
- Single binary is `borg-cli`.
- Primary commands are `borg init` and `borg start`.
- `borg-onboard` is a library server, not a binary.
- `BorgDir` in `borg-core` is source of truth for `~/.borg/*` layout.

## Session-First Model
- Session is the primary LLM interaction unit.
- Ports resolve a long-lived session from `port + conversation_key`.
- Inbound port messages are processed directly as session turns.
- Do not auto-create a task for each inbound message.

## Task Model (Separate)
- Tasks are explicit work graph items.
- Agents create/manage tasks via tools.
- Tasks may own dedicated task-sessions that close on task completion.
- Scheduler/executor loop is for explicit tasks, not baseline chat ingress.

## Storage + Paths
- `~/.borg/config.db` is control-plane/config DB.
- `~/.borg/ltm.db` is directory-backed storage for LTM backend.
- `~/.borg/search.db` is search index storage.
- Avoid scattering path constants; use `BorgDir` accessors.

## DB Notes
- `providers` table stores provider credentials (`openai` currently).
- `port_bindings` stores `port + conversation_key -> session_id (+ optional agent_id)`.
- `port_session_ctx` stores port-owned per-session context snapshots (`port + session_id -> ctx_json`).
- Telegram port refreshes known session context snapshots on startup (best-effort chat/admin hydration).
- Onboarding persists provider key via `POST /api/providers/openai`.

## API/Port Expectations
- `POST /ports/http` should return resolved `session_id` and `reply` (task ID optional).
- `X-Borg-Session-Id` header should be set on successful response.
- Invalid URI inputs at API boundary must fail with structured 400.
- Code-mode filesystem API is `Borg.OS.ls(...)` (not `BorgOs.ls(...)`).

## Runtime Safety
- Initialize tracing before application code in `main`.
- Keep session turn logs and LLM request/response logs explicit.
- Keep task lifecycle transitions explicit for task-subsystem events.
- Convert `deno_core` runtime panics/FFI panics into structured tool errors; do not allow panics to escape runtime boundaries.

## Validate
1. `cargo build`
2. `cargo test -p borg-exec -p borg-api -p borg-ports`
3. `cargo run -p borg-cli -- start` and smoke `POST /ports/http`

## Open TODOs
- Handle provider `context_length_exceeded` failures gracefully instead of surfacing raw 400 errors.
  - Minimum: user-facing guidance to shorten input.
  - Optional improvement: chunk/summarize oversized user input before retry.
- Telegram outbound replies should include a small context-usage indicator (for example `% of context window used`).
