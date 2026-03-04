# Onboarding UI Agent Guide

Scope: `packages/*` web workspaces, chat UX, composer behavior, and i18n.

## Workspace Shape

- Single root Vite app is `apps/borg-admin`.
- Feature packages:
  - `@borg/onboarding`
  - `@borg/dashboard-control`
  - `@borg/dashboard-observability`
  - `@borg/ui`
  - `@borg/i18n`

## UX Model (Important)

- Onboarding is a chat, not a wizard form page.
- Agent prompts appear on left (`Borg`).
- User responses are represented as user-side messages.
- Active controls are rendered in a bottom composer dock.

## Composer/Message Rules

- Provider selection should look like a user reply.
- API key input + connect action should be user-side reply step.
- Supported cloud providers in onboarding are `openai` and `openrouter`.
- Onboarding also supports a local-mode path (no provider credentials) for first-run setup.
- API key save endpoint is `POST /api/providers/:provider` (provider comes from the user choice).
- Preserve sequential reveal:
  - next prompt appears after previous animation/step completion.

## Components

- `@borg/ui` owns shared chat components (`Session`, `Message`, etc.).
- Keep controls message-driven (`choices`, `input`, `actions`).
- Use icon library components (no hand-drawn inline SVG for brand icons).

## i18n

- New user-facing strings go in `@borg/i18n`.
- Avoid hardcoded text in components.

## Validate

1. `bun run build:web`
2. `bun run dev` then open `/onboard`
3. Verify user-side reply behavior for provider + API key
