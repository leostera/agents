# Borg Architecture

Last updated: 2026-02-27

## 1. Purpose
Borg is a local-first, single-binary runtime (`borg-cli`) with durable state in `~/.borg/*`.
It runs long-lived sessions, processes inbound messages through ports, executes agent/tool turns, and keeps explicit tasks as a separate subsystem.

## 2. Core Model
- Session-first ingress: inbound port messages resolve to long-lived sessions.
- Task separation: tasks are explicit work graph items, not automatic per-message artifacts.
- Typed IDs: Borg entities use typed URIs (`borg:*:*`) across APIs, storage, and runtime.
- Single runtime binary: `borg-cli` is the only binary crate.

## 3. Runtime Topology
- `borg init`
- Initializes `BorgDir` storage (`config.db`, memory/search stores).
- Starts `borg-onboard` server (loopback) for onboarding/dashboard assets.
- `borg start`
- Opens `config.db` and memory stores.
- Starts executor loop (`borg-exec`) for queued task processing.
- Starts API server (`borg-api`), including HTTP port ingress and optional Telegram port worker.

## 4. Architecture Loop
```mermaid
flowchart LR
  subgraph Clients
    U[User]
    TG[Telegram]
    W[Web SPA]
  end

  subgraph Runtime
    API[borg-api]
    PORTS[borg-ports]
    EXEC[borg-exec]
    AGENT[borg-agent]
    LLM[borg-llm]
    RT[borg-rt]
    MEM[borg-ltm]
    DB[(config.db)]
  end

  U --> W
  TG --> PORTS
  W --> API
  API --> PORTS
  PORTS --> EXEC
  EXEC --> AGENT
  AGENT --> LLM
  AGENT --> RT
  AGENT --> MEM
  EXEC --> DB
  API --> DB
  MEM --> DB
```

## 5. Session Flow (Port -> Turn)
```mermaid
sequenceDiagram
  participant P as Port (HTTP/Telegram)
  participant DB as config.db
  participant X as ExecEngine
  participant S as Session (borg-agent)
  participant L as Provider (borg-llm)

  P->>DB: resolve_port_session(port, conversation_key, requested session/agent)
  DB-->>P: session_id (+ optional bound agent_id)
  P->>X: process_port_message(user_key, text, metadata, session_id)
  X->>S: load/build session context + agent spec
  S->>L: completion + tool calls
  L-->>S: assistant/tool outputs
  S-->>X: turn result (reply + tool summaries)
  X-->>P: session_id + reply
```

## 6. Task Lifecycle (Explicit Subsystem)
Tasks are queued and processed independently from normal port ingress.

```mermaid
stateDiagram-v2
  [*] --> queued
  queued --> running: claimed by worker
  running --> completed: success terminal event
  running --> failed: error/panic terminal event
  failed --> queued: retry/requeue policy
  completed --> [*]
  failed --> [*]
```

Notes:
- User chat ingress usually runs directly as a session turn via ports.
- Explicit queued tasks still exist for decomposed/managed work and startup recovery (`requeue_running_tasks`).
- A task can own a dedicated task session; root conversation sessions remain long-lived.

## 7. Onboarding and Web Delivery
`borg-onboard` serves prebuilt SPA artifacts from `packages/borg-app/dist` and fails loudly if missing.

```mermaid
flowchart TD
  A[bun run build:web] --> D[packages/borg-app/dist]
  D --> O[borg init -> borg-onboard]
  O --> R1[GET /onboard]
  O --> R2[GET /dashboard]
  O --> R3[GET /assets/app.js + app.css]
  O --> P[POST /api/providers/:provider]
  P --> C[(config.db providers + runtime preferred_provider)]
```

Current UI state:
- Single SPA root is `apps/borg-admin`.
- `/dashboard` renders the dashboard shell.
- `/onboard` route mounts `@borg/onboarding`, a chat-first setup flow for provider/local mode, assistant creation, and first channel connection.

## 8. Port Delivery Order
```mermaid
sequenceDiagram
  participant T as Telegram inbound
  participant C as Command Registry
  participant X as ExecEngine
  participant S as Session Turn
  participant O as Telegram outbound

  T->>C: check slash command
  alt Command recognized
    C->>X: run explicit command action
    X-->>O: command reply
  else Not a command
    T->>X: process_port_message
    X->>S: run agent turn
    S-->>X: reply + tool call summaries
    X-->>O: tool action messages, assistant reply, context usage %
  end
```

## 9. Storage Layout
`borg-core::BorgDir` is the source of truth for path layout.

- `~/.borg/config.db`
- tasks, deps, task_events
- sessions, session_messages
- users
- providers
- port_settings
- port_bindings (`port + conversation_key -> session_id + optional agent_id`)
- port_session_ctx (`port + session_id -> ctx_json`)
- agent_specs
- policies, policies_use
- `~/.borg/ltm.db`
- fact store + entity graph backing data
- `~/.borg/search.db`
- Tantivy search index

## 10. HTTP Surface (Current)
- Runtime/status:
- `GET /health`
- Port ingress:
- `POST /ports/http`
- Returns `session_id`, `reply`, optional `task_id`; sets `X-Borg-Session-Id` on success.
- Task and memory reads:
- `GET /tasks`, `GET /tasks/:id`, `GET /tasks/:id/events`, `GET /tasks/:id/output`
- `GET /memory/search`, `GET /memory/entities/:id`
- CRUD-style control plane:
- providers, policies, agent specs, users, sessions, session messages
- port settings, port bindings, port session context

## 11. Build and Runtime Assumptions
- Web build: `bun run build:web`.
- Rust build: `cargo build` (or `cargo build -p borg-cli`).
- Onboarding backend expects `packages/borg-app/dist` and does not silently fallback when missing.
- Provider precedence is env-first (`BORG_LLM_PROVIDER`) over persisted runtime preferred provider.

## 12. Non-Goals (v0)
- Distributed scheduling and multi-node coordination.
- Multi-tenant hard isolation.
- Stable long-term API compatibility guarantees pre-v1.
- Fully finished onboarding chat UX in the dashboard SPA (currently in transition).
