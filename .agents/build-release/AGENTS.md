# Build and Release Agent Guide

Scope: builds, workspace wiring, and command expectations.

## Canonical Build Commands

- Web build:
  - `bun run build:web`
- Rust build:
  - `cargo build`
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
- Workspace-level Cargo config forces bundled RocksDB compilation via:
  - `.cargo/config.toml` with `ROCKSDB_COMPILE=1`
  - This avoids accidental linking to missing system `librocksdb.a` when global Cargo env overrides are present.

## Commit Hygiene

- Use conventional commits.
- Prefer small commits grouped by concern:
  - `feat(...)`
  - `fix(...)`
  - `refactor(...)`
  - `docs(...)`

## Pre-Push Checklist

1. `bun run build:web` succeeds.
2. `cargo build` succeeds.
3. `cargo test -p borg-exec -p borg-api -p borg-ports` succeeds for runtime-path changes.
4. No unrelated local breakages.
