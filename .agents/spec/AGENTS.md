# Spec Agent Guide

Scope: keeping `ARCHITECTURE.md` aligned with implementation and direction.

## Priority Areas (Must Stay Current)

- Core subsystem emphasis:
  - Memory
  - Executor
  - Ports
  - Agent Runtime
- Onboarding chat-form model and user-side replies
- Build/runtime assumptions:
  - single binary (`borg-cli`)
  - root SPA build
  - fail-loud dist loading in onboarding backend
  - session-first ingress (`port -> session turn`), with tasks as separate explicit subsystem

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
- onboarding flow semantics
- scheduler or runtime lifecycle behavior
- session/task ownership model (especially `port_bindings` and task-session semantics)

## Quality Bar

- Use clear title hierarchy.
- Keep sections concise and implementation-oriented.
- Note explicit non-goals to protect v0 scope.
