# AGENTS Router

This file is the entrypoint for project-specific agent guidance.

The `AGENTS.md` files in this repo are maintained alongside the code and should be updated when behavior or contracts change.

Use it as a router: pick the most relevant existing AGENTS file before making changes.

## Routing Table

1. Agent runtime, sessions, and event model:
   - [`crates/borg-agent/AGENTS.md`](crates/borg-agent/AGENTS.md)
2. LLM providers, typed tools, and model-facing APIs:
   - [`crates/borg-llm/AGENTS.md`](crates/borg-llm/AGENTS.md)
3. Evals, runner workflow, cargo-evals, and macro-generated suite wiring:
   - [`crates/borg-evals/AGENTS.md`](crates/borg-evals/AGENTS.md)
   - this route also covers `borg-macros` work for `#[suite]`, `#[eval]`, `#[grade]`, and `#[derive(AgentTool)]`
4. Workspace-wide changes without a more specific AGENTS file:
   - use this root file and inspect nearby crate code directly

## Global Rules (Apply Everywhere)

- Keep Rust code idiomatic and struct-oriented.
- Prefer named constants over magic values.
- Initialize tracing before other app logic.
- Prefer error propagation with `?` where possible.
- When updating paths in documentation never use absolute paths -- always use paths relative to the repository root
- Git hooks are managed via Cargo Husky user hooks in `.cargo-husky/hooks/`; do not reintroduce `.husky` or `core.hooksPath` overrides unless explicitly intended

## Fast Start Checklist

1. Identify the domain area.
2. Read the matching AGENTS file if one exists.
3. Implement changes.
4. Run required builds.
5. Update affected AGENTS files if behavior or contracts changed.
