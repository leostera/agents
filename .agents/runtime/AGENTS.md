# Runtime Agent Guide

Scope: Rust runtime behavior, actor turns, explicit tasks, storage wiring, and server lifecycle.

## Current Runtime Contracts
- Single binary is `borg-cli`.
- Primary commands are `borg init` and `borg start`.
- `borg-onboard` is a library server, not a binary.
- `BorgDir` in `borg-core` is source of truth for `~/.borg/*` layout.
- Embedded local inference v0 lives in `borg-infer` (separate from `borg-llm` provider orchestration).
- `borg-llm` exposes a Hugging Face GGUF downloader that caches model files under `~/.borg/models/<org>/<model>/<revision>/...`.

## Actor-Only Model
- Actor is the primary LLM interaction unit.
- Ports resolve a long-lived actor from `port + conversation_key`.
- Inbound port messages are processed directly as actor turns.
- Do not auto-create a task for each inbound message.
- There is no session identity in runtime paths (`actor_id` only).

## Task Model (Separate)
- Tasks are explicit work graph items.
- Agents create/manage tasks via tools.
- Tasks are assigned/reviewed by actor ids.
- Scheduler/executor loop is for explicit tasks, not baseline chat ingress.
- TaskGraph MCP tooling is available by default via `TaskGraph-*` tools from `borg-taskgraph`.
- `borg start` launches `TaskGraphSupervisor` as a background runtime monitor for task status transitions.

## Storage + Paths
- `~/.borg/config.db` is control-plane/config DB.
- `~/.borg/ltm.db` is directory-backed storage for LTM backend.
- `~/.borg/search.db` is search index storage.
- Avoid scattering path constants; use `BorgDir` accessors.

## DB Notes
- `providers` table stores provider credentials (`openai`, `openrouter`).
- `port_settings` stores runtime defaults under `port=runtime` (for example `preferred_provider`).
- `port_bindings` stores `port + conversation_key -> actor_id` plus actor context snapshot/reasoning fields.
- `taskgraph_*` tables in `config.db` store durable task DAG state, comments, and audit events.
- `schedule_jobs` and `schedule_job_runs` in `config.db` store durable scheduler jobs/runs.
- Telegram port refreshes known actor context snapshots on startup (best-effort chat/admin hydration).
- Onboarding persists provider settings via GraphQL mutations and updates runtime preferred provider.
- Provider precedence is env-first: `BORG_LLM_PROVIDER` overrides persisted `runtime/preferred_provider`.
- When preferred provider is `openrouter`, transcription falls back to OpenAI credentials (or returns a clear missing-key error).

## API/Port Expectations
- `POST /ports/http` should return resolved `actor_id` and `reply` (task ID optional).
- `X-Borg-Actor-Id` header should be set on successful response.
- `GET /health` should remain available for readiness/liveness checks.
- Control-plane CRUD/read APIs are GraphQL-first under `/gql` (plus `/gql/ws`, `/gql/graphiql`).
- Keep runtime ingress REST surface minimal outside GraphQL (`/ports/http`, `/health`).
- Code-mode filesystem API is `Borg.OS.ls(...)` (not `BorgOs.ls(...)`).
- Code-mode module resolution is embedded (no host `node` dependency): dynamic imports may use `npm:` and `jsr:` specifiers, with cache/state under `~/.borg/codemode` and `node_modules` in `~/.borg/codemode/node_modules`.
- Telegram command `/model` supports:
  - `/model` to show current `actor_id` + model for the chat actor.
  - `/model <model_name>` to persist model on the resolved actor.
- GraphQL policy surfaces were removed (`policy`, `policies`, `Policy`, `PolicyUse`, `Node::Policy`).
- GraphQL actor upsert no longer requires `defaultBehaviorId`; actor config is actor-owned.
- Telegram outbound now uses `ParseMode::Html` with safe formatting for markdown-like bold/italics/links and bullet-style lists.
- Telegram tool-action progress messages render as HTML: `<i>{hint or tool}</i> ({elapsed})` plus spoiler-wrapped JSON args details.
- Runtime toolchain now merges CodeMode + ShellMode + Memory + BorgFS + TaskGraph + Apps-listCapabilities in actor turns.
- Runtime toolchain now includes executable Schedule tools (`Schedule-*`) for scheduler job CRUD/list-runs.
- TaskGraph tool surface includes `TaskGraph-listTasks` for top-level task pagination.
- `borg tools` command outputs decoded JSON payloads (no `ToolResultData` envelope in CLI output).
- CLI command routing: taskgraph commands live under `borg task <cmd>` (including `delete`), and memory tool commands live under `borg memory <cmd>`.
- Agent-visible tool specs now include active DB app capabilities (in addition to default runtime tools), so the LLM can call those capability tools directly by name.
- Default app seeding includes `borg:app:clockwork-system` with Clockwork capabilities mirrored from runtime tool specs.
- App `available_secrets` are exported into CodeMode env verbatim by the same key name (no `APP_` prefix translation).
- `borg start` launches a Clockwork supervisor loop (1s poll cadence) as scheduler runtime scaffolding.
- Local inference smoke commands:
  - `borg infer models` lists hardcoded GGUF entries from `borg-infer`.
  - `borg infer run <model_id> <input_text> [--gguf <path>]` runs embedded generation with streaming + Ctrl-C cancel.
  - `borg providers set default embedded` updates runtime preferred provider settings (routing integration is follow-up).

## Runtime Safety
- Initialize tracing before application code in `main`.
- Keep actor turn logs and LLM request/response logs explicit.
- Keep task lifecycle transitions explicit for task-subsystem events.
- Convert `deno_core` runtime panics/FFI panics into structured tool errors; do not allow panics to escape runtime boundaries.

## Validate
1. `cargo build`
2. `cargo test -p borg-exec -p borg-gql -p borg-ports`
3. `cargo run -p borg-cli -- start` and smoke `POST /ports/http`
4. `cargo test -p borg-infer`

## Open TODOs
- Telegram outbound replies should include a small context-usage indicator (for example `% of context window used`).
