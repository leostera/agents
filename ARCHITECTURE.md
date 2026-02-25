# Borg Architecture

## 1. Purpose
Borg is a single-binary runtime (`borg-cli`) that receives events, schedules tasks, runs agent+tool loops, persists durable memory/state, and exposes a lightweight control plane.

Goals:
- event ingestion to task execution
- deterministic task lifecycle persistence
- durable long-term memory
- observable behavior via structured tracing

## 2. System Shape
Runtime process (`borg start`) includes:
- HTTP ingress/control API
- scheduler + worker execution loop
- agent runtime + tool orchestration
- long-term memory service

`borg init` initializes local state and starts onboarding.

## 3. High-Level Architecture
```mermaid
flowchart LR
  E[External Event / User Input] --> P[Ports / HTTP Ingress]
  P --> X[Executor + Scheduler]
  X --> A[Agent Runtime]
  A --> T[Tool Runner]
  A --> M[Memory Service]
  A --> X
  X --> D[(config.db)]
  M --> L[(ltm.db)]
  C[Control Plane API/UI] --> P
  C --> X
  C --> M
```

## 4. Core Runtime Loop
```mermaid
sequenceDiagram
  participant U as User/Event
  participant H as HTTP/Port
  participant E as Executor
  participant G as Agent
  participant R as Tool Runner
  participant M as Memory

  U->>H: inbound event
  H->>E: enqueue task
  E->>E: claim runnable task
  E->>G: run session turn
  G->>M: search/read context
  G->>R: tool call(s)
  R-->>G: tool result(s)
  G->>M: write/update facts/entities
  G-->>E: completion / error / follow-up
  E->>E: persist task/event transitions
```

## 5. Task Lifecycle
```mermaid
stateDiagram-v2
  [*] --> queued
  queued --> running
  running --> succeeded
  running --> failed
  running --> blocked
  blocked --> queued
  queued --> canceled
  blocked --> canceled
  failed --> queued: retry policy
```

## 6. Data and Storage
`~/.borg/*` (via `BorgDir`):
- `config.db`: task/control/session/provider state
- `ltm.db/`: memory backing data
- `logs/`: runtime logs

Durability model:
- runtime process may restart
- state survives via `config.db` + `ltm.db`

## 7. API Surface (v0)
Control endpoints:
- `GET /health`
- `GET /tasks`
- `GET /tasks/:id`
- `GET /tasks/:id/events`
- `GET /memory/search`
- `GET /memory/entities/:id`

Onboarding endpoints:
- `GET /onboard`
- `POST /api/providers/openai`

## 8. Onboarding Architecture
Chat-first onboarding uses a message-driven UI model:
- Borg prompts in feed
- user replies through composer controls
- provider credentials persisted in `config.db`

```mermaid
flowchart TD
  A[Borg onboarding prompt] --> B[User composer action]
  B --> C[User message persisted]
  C --> D[POST /api/providers/openai]
  D --> E[(config.db providers)]
  E --> F[Borg success prompt]
```

## 9. Observability
Tracing is initialized at process start and used across:
- CLI/runtime lifecycle
- scheduler/executor transitions
- agent turns and tool execution
- DB and memory operations
- onboarding/web flows
- integration/e2e test harnesses (including tool call/result traces)

## 10. Boundaries and Non-Goals (v0)
In scope:
- single-node runtime
- dynamic task execution with tool loops
- durable local state and memory

Out of scope:
- multi-node distributed scheduling
- full auth/tenant model
- advanced orchestration policies
