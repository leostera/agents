# AGENTS Router

This file is the entrypoint for project-specific agent guidance.
The `.agents/*` files are Codex-maintained project memory and should be updated by Codex periodically as behavior/contracts evolve.

Use it as a router: pick the most relevant sub-agent doc below before making changes.

## Routing Table

1. Runtime + backend orchestration:
   - [`.agents/runtime/AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/.agents/runtime/AGENTS.md)
2. Onboarding chat UX + web packages:
   - [`.agents/onboarding-ui/AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/.agents/onboarding-ui/AGENTS.md)
3. Build, workspace, and release workflow:
   - [`.agents/build-release/AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/.agents/build-release/AGENTS.md)
4. Product spec + architecture consistency:
   - [`.agents/spec/AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/.agents/spec/AGENTS.md)
5. Maintenance protocol for keeping all AGENTS docs current:
   - [`.agents/maintenance/AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/.agents/maintenance/AGENTS.md)

## Global Rules (Apply Everywhere)

- Keep Rust code idiomatic and struct-oriented.
- Keep `borg-cli` as the only binary crate.
- Prefer named constants over magic values.
- Initialize tracing before other app logic.
- Prefer error propagation with `?` where possible.
- Runtime model is session-first:
  - ports feed long-lived sessions directly
  - tasks are explicit and separate from normal chat ingress
- Run both builds after substantial changes:
  - `bun run build:web`
  - `cargo build`

## Fast Start Checklist

1. Identify domain area (runtime / onboarding UI / build / spec).
2. Read the matching AGENTS subfile.
3. Implement changes.
4. Run required builds.
5. Update affected AGENTS subfiles if behavior/contracts changed.
