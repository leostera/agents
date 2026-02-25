# AGENTS Maintenance Guide

Scope: how to keep this AGENTS system up to date automatically and periodically.

## Automatic Maintenance (Per Change)

After any meaningful change:
1. Identify affected domain(s):
   - runtime
   - onboarding-ui
   - build-release
   - spec
2. Update corresponding `.agents/*/AGENTS.md` with:
   - changed contracts
   - changed commands
   - changed paths/endpoints
3. If cross-domain impact exists, update root [`AGENTS.md`](/Users/leostera/Developer/github.com/leostera/borg/AGENTS.md) routing/checklist.

## Periodic Maintenance (Weekly / Release Cut)

Run this sweep:
1. Validate all command snippets still work.
2. Validate crate/package paths still match repository layout.
3. Compare `SPEC_V0.md` against AGENTS docs for drift.
4. Prune stale notes and add missing decisions.

## Drift Signals (Must Update)

- Build command changed.
- Endpoint changed.
- New crate/package introduced.
- Onboarding UX state model changed.
- Runtime path/storage semantics changed.

## Ownership Rule

Any contributor touching behavior in one domain must update that domain's AGENTS doc in the same PR/commit set.
