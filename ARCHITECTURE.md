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
  subgraph Clients
    U[Users]
    W[Web UI Dashboard]
    S[External Systems]
  end

  subgraph APIPlane
    BAPI[borg_api planned]
    ONB[Onboarding Web]
  end

  subgraph Runtime
    PORTS[Ports Agent Entry]
    EXEC[Executor Scheduler]
    AGENT[Agent Runtime]
    TOOLS[Tool Runner]
    RT[Runtime Sandbox]
    LTM[Memory Service]
  end

  CFG[(config.db)]
  MEM[(ltm.db)]

  U --> W
  W --> BAPI
  U --> ONB
  S --> PORTS
  BAPI --> EXEC
  BAPI --> LTM
  BAPI --> CFG
  ONB --> CFG
  PORTS --> EXEC
  EXEC --> AGENT
  AGENT --> TOOLS
  TOOLS --> RT
  AGENT --> LTM
  AGENT --> EXEC
  EXEC --> CFG
  LTM --> MEM
```

Future split:
- `borg-api` (planned crate) will expose the full system/control-plane API used by web UI/dashboard.
- ports remain agent-facing ingress/egress entrypoints for agent conversations and event ingestion.

## 4. Core Runtime Loop
```mermaid
sequenceDiagram
  participant P as Port
  participant X as Executor
  participant A as Agent
  participant T as Tool Runner
  participant SB as Sandbox
  participant M as Memory
  participant DB as config.db

  P->>X: enqueue task(event payload)
  X->>DB: persist task + task_created event
  X->>X: claim runnable task
  X->>A: start session turn(task + context)
  A->>M: retrieve context(search/get)
  A->>T: tool_call(name, args JSON)
  T->>SB: execute/search capability
  SB-->>T: raw execution result
  T-->>A: tool_result(JSON or error)
  A->>M: project facts/entities/relations
  A-->>X: completed / idle / failed + follow-ups
  X->>DB: update task status + append task events
  X-->>P: optional egress message/event
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

Architecture direction:
- these routes will move behind/through `borg-api` as the dedicated API surface for control-plane + system introspection.
- ports are not the control-plane API; ports are exclusively entrypoints to talk to agents.

```mermaid
flowchart LR
  UI[Web UI Dashboard] --> API[borg_api planned]
  OP[Operators Automation] --> API
  API --> CP[Control Plane Endpoints]
  API --> MS[Memory Query Endpoints]
  API --> TS[Task Event Endpoints]

  EV[External Event Sources] --> PORT[Ports]
  PORT --> AG[Agent Entry Pipeline]

  note1[Ports are agent ingress egress only]
  note2[borg_api is system control plane surface]
  PORT -.-> note1
  API -.-> note2
```

## 8. Onboarding Architecture
Chat-first onboarding uses a message-driven UI model:
- Borg prompts in feed
- user replies through composer controls
- provider credentials persisted in `config.db`

```mermaid
flowchart TD
  USER[User] --> SPA[Onboarding SPA]
  SPA --> CHAT[Chat Composer + Feed]
  CHAT --> API[Onboarding Backend]
  API --> PROVIDER[POST /api/providers/openai]
  PROVIDER --> CFG[(config.db providers)]
  CFG --> API
  API --> CHAT
  CHAT --> USER
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
