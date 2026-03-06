# Build and Release Agent Guide

Scope: builds, workspace wiring, and command expectations.

## Canonical Build Commands

- Web build:
  - `bun run build:web`
  - `bun run build:stage`
- Docs site build:
  - `bun run build:www`
- Standalone docs site build:
  - `bun run build:www-standalone`
- Rust build:
  - `cargo build`
  - Fast local path (auto-uses `sccache` when installed): `./cargo build -p borg-cli`
- Full local build:
  - `bun run build && ./cargo build -p borg-cli` (or equivalent sequence)

## Dev Commands

- Web dev:
  - `bun run dev:web`
  - `bun run dev:stage`
- All local web+docs dev servers (single entrypoint):
  - `bun run dev`
  - Runs:
    - Vite app server
    - Stage playground on port `5174`
    - `www/dev.openborg` on port `4321`
    - `www/dev.openborg.docs` on port `4322`
    - Storybook on port `6006`
- Docs site dev:
  - `bun run dev:www`
- Standalone docs site dev:
  - `bun run dev:www-standalone`
- Workspace tests:
  - `bun test ./packages --pass-with-no-tests`
- GraphQL artifacts:
  - `bun run graphql:codegen`
- Fast typecheck lane:
  - `bun run typecheck:fast` (tries `tsgo`, falls back to `tsc -b`)
- CLI runtime:
  - `cargo run -p borg-cli -- init`
  - `cargo run -p borg-cli -- start`

## Build Guarantees

- Docs projects live under:
  - `www/dev.openborg`
  - `www/dev.openborg.docs`
- Root web build must produce `packages/borg-app/dist`.
- Backend onboarding server expects dist assets and fails loudly if missing.
- `borg-codemode` build depends on SDK artifacts at `packages/borg-agent-sdk/dist/borg-agent-sdk.min.js`.
  - If missing, run: `bun run --cwd packages/borg-agent-sdk build`.
- Workspace-level Cargo config forces bundled RocksDB compilation via:
  - `.cargo/config.toml` with `ROCKSDB_COMPILE=1`
  - This avoids accidental linking to missing system `librocksdb.a` when global Cargo env overrides are present.
- Workspace-level Cargo config also pins `PKG_CONFIG_PATH` to a stable value.
  - This prevents repeated native crate invalidation (`libz-sys`, `libgit2-sys`) caused by shell-specific `PKG_CONFIG_PATH` drift.
- Dev profile keeps dependency debuginfo disabled:
  - `[profile.dev.package."*"] debug = 0`
  - This shortens local rebuilds while preserving debuginfo for workspace crates.
- Embedded local inference compiles via `crates/borg-infer` using `llama-cpp-2` unconditionally.

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
