# Build and Release Agent Guide

Scope: builds, workspace wiring, and command expectations.

## Canonical Build Commands

- Web build:
  - `bun run build:web`
- Rust build:
  - `cargo build -p borg-cli`
- Full local build:
  - `bun run build && cargo build -p borg-cli` (or equivalent sequence)

## Dev Commands

- Web dev:
  - `bun run dev`
- CLI runtime:
  - `cargo run -p borg-cli -- init`
  - `cargo run -p borg-cli -- start`

## Build Guarantees

- Root web build must produce `packages/borg-app/dist`.
- Backend onboarding server expects dist assets and fails loudly if missing.

## Commit Hygiene

- Use conventional commits.
- Prefer small commits grouped by concern:
  - `feat(...)`
  - `fix(...)`
  - `refactor(...)`
  - `docs(...)`

## Pre-Push Checklist

1. `bun run build:web` succeeds.
2. `cargo build -p borg-cli` succeeds.
3. No unrelated local breakages.
