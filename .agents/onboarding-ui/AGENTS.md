# Onboarding UI Agent Guide

Scope: `apps/devmode` + `apps/stage` web UIs, local-first workspace UX, and shared frontend packages.

## Workspace Shape

- App entrypoints:
  - `apps/devmode` (workspace/task/agent control plane)
  - `apps/stage` (actor graph + mailbox playground)
- Feature packages:
  - `@borg/graphql-client`
  - `@borg/ui`
  - `@borg/i18n`

## UX Model (Important)

- DevMode is workspace-first and local-first.
- Workspace creation captures namespace + project root and queues scanner actor bootstrap.
- Tasks, agent profiles, comments, and activity logs persist locally first, then sync side effects to GraphQL runtime.

## Runtime Rules

- Runtime endpoint is discovered via `resolveDefaultBaseUrl()` from `@borg/graphql-client`.
- Agent/scanner registration uses GraphQL actor upserts.
- UI must remain usable when runtime is unavailable; preserve local state and surface health clearly.

## Components

- `@borg/ui` owns reusable primitives and should remain the default source for controls/layout pieces.
- Keep task/agent activity timelines explicit and inspectable.
- Avoid hand-rolled duplicate primitive components in `apps/devmode`.

## i18n

- New user-facing strings go in `@borg/i18n`.
- Avoid hardcoded text in components.

## Validate

1. `bun run typecheck:fast`
2. `bun run build:web`
3. `cargo build`
