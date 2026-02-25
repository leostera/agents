# Runtime Agent Guide

Scope: Rust runtime behavior, task execution, storage wiring, and server lifecycle.

## Current Project Decisions

- Single binary is `borg-cli`.
- Primary commands are `borg init` and `borg start`.
- `borg-onboard` is a library server, not a binary.
- `BorgDir` in `borg-core` is source of truth for `~/.borg/*` layout.

## Storage + Paths

- `~/.borg/config.db` is control-plane/config DB.
- `~/.borg/ltm.db` is directory-backed storage for LTM backend.
- Avoid scattering path constants; use `BorgDir` accessors.

## Runtime Safety

- Initialize tracing before application code in `main`.
- Keep scheduler loop robust and noisy with structured logs.
- Keep task lifecycle transitions explicit.

## Onboarding Server Contract

- Server must fail loudly if web dist assets are missing.
- Dist path currently expected by backend: `packages/borg-app/dist`.
- No silent fallback inline assets in production path.

## DB Notes

- `providers` table stores provider credentials (`openai` currently).
- Onboarding POST endpoint persists API key:
  - `POST /api/providers/openai`

## Validate

1. `cargo build -p borg-cli`
2. `cargo run -p borg-cli -- init` (smoke check)
3. `cargo run -p borg-cli -- start` (smoke check)
