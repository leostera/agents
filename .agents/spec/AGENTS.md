# Spec Agent Guide

Scope: keeping `ARCHITECTURE.md` aligned with implementation and direction.

## Priority Areas (Must Stay Current)

- Core subsystem emphasis:
  - Memory
  - TaskGraph
  - Executor
  - Ports
  - Agent Runtime
- Onboarding chat-form model and user-side replies
- Build/runtime assumptions:
  - single binary (`borg-cli`)
  - root SPA build
  - fail-loud dist loading in onboarding backend
- actor-first ingress (`port -> actor turn`), with tasks as separate explicit subsystem
- taskgraph MCP contract (`borg.taskgraph`) for durable agent task DAGs and queue semantics

## Diagram Expectations

When updating spec, maintain Mermaid diagrams for:
- architecture loop
- task lifecycle
- onboarding flow
- delivery order

## Update Triggers

Update `ARCHITECTURE.md` when any of these change:
- API contracts/endpoints
- crate/package ownership boundaries
- taskgraph MCP resources/tools, invariants, or queue/review semantics
- onboarding flow semantics
- scheduler or runtime lifecycle behavior
- actor/task ownership model (especially `port_bindings` actor routing semantics)

## Quality Bar

- Use clear title hierarchy.
- Keep sections concise and implementation-oriented.
- Note explicit non-goals to protect v0 scope.
