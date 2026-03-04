# Changelog

## 2026-03-04 - Typed Boundary Completion, Chat-First Onboarding Ship, Frontend Reset


- Memory/core/taskgraph/borgfs/exec typed migrations continued with JSON
contract removals, and that completion work gave users more robust runtime
correctness at tool and event boundaries.

- CLI/tests were aligned with typed envelopes during the same push, and that
test-contract discipline gave users higher confidence that command behavior
matches runtime expectations.

- Hugging Face GGUF pull command support was added as local inference needs
matured, which gave users a faster path to acquiring local model artifacts.

- Chat-first onboarding with Telegram handoff shipped while
dashboard/onboarding serving paths were reworked, and that integration learning
gave users a shorter time-to-first-useful-actor.

- Frontend architecture reset work introduced `apps/borg-admin`, GraphQL-first
package layout, and router-shell refactors, which gave users a clearer
long-term path for scalable admin UX.

## 2026-03-03 - Hardening and Expansion: Typed Runtime Push, BorgFS, Embedded Inference


- The codebase began a wide typed-boundary migration across
agent/exec/ports/tools, and the learning was that implicit JSON contracts were
a major long-term risk, which gave users safer, more consistent tool execution
behavior.

- Agent-spec API/UI surfaces were removed as actor-first direction solidified,
and that hard cut gave users a cleaner model with fewer overlapping
abstractions.

- BorgFS runtime tools, CLI, and dashboard explorer were added with audio
ingest/transcription flow, and that expansion gave users concrete file/audio
automation capabilities.

- Embedded local inference v0 shipped and CLI groups moved from `llm` to
`infer`, and that packaging learning gave users clearer local inference
workflows.

- Typed request DTO migrations reached codemode, shellmode, clockwork,
taskgraph, and app discovery paths, which gave users more reliable behavior
across advanced tool ecosystems.

- Operational commands like session clearing and env passthrough were added
while build fixes continued, and that practical ops focus gave users better
maintenance and debugging control.

## 2026-03-02 - Actor-First Delivery: Mailbox Durability, Behaviors, Local Providers


- Actor storage, mailbox APIs, and replay/ack/fail semantics shipped, and the
key learning was that recovery semantics must be explicit, which gave users
runtime resilience across restarts and partial failures.

- Port binding resolution became actor-aware with compatibility fallback paths,
and that migration strategy gave users safer adoption without immediate
breakage.

- Control UI/API expanded to actor CRUD, actor assignment, and behavior
management, and that product shift gave users explicit operational handles for
autonomous roles.

- The "agents" navigation was replaced by behaviors, reflecting the learning
that actor+behavior is clearer than overloaded agent terminology, which gave
users a more legible control plane.

- Local provider support for LM Studio and Ollama was added while provider
IDs/kinds were normalized, which gave users stronger local-first model options.

- Discord scaffolding landed alongside actor-first cleanup, and that channel
expansion gave users a near-term path to broader messaging integrations.

## 2026-03-01 - Runtime Simplification: Capability Loading and Shell Mode


- Task queue/task-model surfaces were reduced in the main runtime path, and the
learning was to avoid accidental complexity in chat-first flows, which gave
users more direct and predictable message handling.

- Capabilities began loading from DB-backed app state instead of hardwired
paths, and that architecture shift gave users more flexible toolchain behavior
without code edits for every capability change.

- Shell mode was added and immediately fixed to include stdout/stderr in
results, and that fast loop gave users command execution that models can
actually reason about.

- Actorized supervisor/port reconciliation work started in earnest, and that
progression gave users a foundation for more reliable multi-actor channel
handling.

## 2026-02-28 - Control Plane Pivot: URI Contracts, Apps/Capabilities,
Observability


- A breaking migration moved ports/control APIs to URI-centric contracts, and
the learning was that typed identity must be explicit across layers, which gave
users more predictable cross-API references and cleaner integrations.

- Codemode tooling was reframed as apps plus capabilities with secrets/grants
semantics, and that conceptual pivot gave users a clearer model for what can
run, under which app, and with which permissions.

- Runtime and DB layers were pushed further into SQLx macro coverage while
tests became stricter, and that rigor gave users a more stable platform under
rapid feature growth.

- LLM call persistence and tracing/session views were introduced as first-class
product surfaces, reflecting the learning that observability is user value,
which gave users real debugging and trust tooling.

- Provider model-default UX was refined repeatedly (details pages and better
selectors), and that UX iteration gave users faster model configuration with
fewer invalid states.

- The memory subsystem was renamed from `borg-ltm` to `borg-memory` and aligned
with MCP-style tooling docs, and that naming/contract clarity gave users a more
understandable mental model for memory operations.

## 2026-02-27 - Productization: SQLx, Provider UX, Dynamic Port Supervision


- Legacy onboarding paths were removed and web flows were consolidated, and
that cleanup gave users a less confusing path to configure and run the system.

- The DB stack migrated toward SQLx/SQLite with migration hardening, reflecting
the learning that compile-time query checks and startup safety matter early,
which gave users more predictable upgrades and fewer runtime schema surprises.

- Provider management evolved from basic CRUD to enabled flags, usage
summaries, and branded connect UX, and that iteration gave users practical
provider operations instead of low-level config editing.

- Dashboard routing, memory explorer surfaces, and shared API client patterns
were expanded, and that frontend/backend integration gave users visible
control-plane insight into memory and system state.

- Dynamic port supervision and device-code auth support were added while
pre-commit/build hygiene tightened, which gave users more reliable runtime
channel behavior and fewer regressions.

## 2026-02-26 - Operational Control: Commands, Memory Tools, RFD Discipline


- The team introduced RFD/PID templates while shipping features, learning that
design docs must evolve in lockstep with code, which gave users more coherent
behavior across fast-moving changes.

- Telegram command handling moved from ad hoc logic to a registry/dispatcher
model, and that learning gave users reliable operational commands like
compact/participants/context/reset.

- Memory APIs were reframed from SDK-side convenience to first-class runtime
tools, and that shift gave users more consistent memory behavior across
channels and tool calls.

- Port bindings/context APIs and policy CRUD endpoints were added while runtime
context persistence was hardened, which gave users stronger control over
routing, authorization, and session behavior.

- Voice transcription and provider flexibility (including OpenRouter and model
switching paths) were added while compacting context logic improved, which gave
users better multimodal input and longer useful sessions.

## 2026-02-25 - Foundation: Bootstrap, Sessions, First Onboarding


- The project was bootstrapped as a multi-crate local-first runtime, and the
team quickly learned to keep `borg-cli` as the single operational entrypoint,
which gave users a clear `init/start` path instead of fragmented startup steps.

- Onboarding UX iterated rapidly from static controls into chat-embedded
provider/key interactions, and that learning produced a setup flow users can
complete inside conversation without context switching.

- Runtime work converged on session-first ingress after early task-oriented
experiments, which gave users durable long-lived conversations and explicit
continuity via `X-Borg-Session-Id`.

- Memory internals were split and tested (fact store, graph projection, restart
behavior), and that engineering discipline gave users persistent memory/search
behavior that survives process restarts.

- API/ports/executor boundaries were modularized and Telegram scaffolding
landed, and that subsystem separation gave users a first practical channel
(`/start`) while keeping room for more ports.

- Architecture/spec docs were rewritten to match real runtime behavior, and
that documentation hygiene gave users and contributors a more trustworthy map
of how the system actually works.

## 2026-02-24 - Hello World!


- Users can run a local-first runtime with durable sessions, actor-first
routing, and multiple channel surfaces.

- Users can configure cloud and local providers, including model defaults and
local inference workflows.

- Users can use memory, shell, code/task tools, and filesystem/audio flows
through progressively typed runtime contracts.

- Users can inspect runtime behavior with persisted traces, session views,
memory explorers, and growing admin surfaces.

- Users benefit from a codebase that increasingly couples design docs
(RFDs/architecture) with implementation, reducing drift as capabilities expand.
