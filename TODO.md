# Borg TODO

## 1) Finish `borg init` onboarding flow (highest priority)
- Complete end-to-end `/onboard` web flow so `borg init` reliably reaches a usable runtime state.
- Persist LLM provider config through `borg-db` (provider key + model/provider defaults).
- Add onboarding step to configure the first port (Telegram only for now):
  - bot token
  - initial port binding defaults
- Ensure onboarding writes all required DB records for first-run success.
- Add a final onboarding completion check so we can fail loudly if setup is incomplete.

## 2) Make `borg start` serve a usable local dashboard
- Finish dashboard build and integrate into local runtime serving path.
- Ensure dashboard assets are available and loaded from the expected dist path.
- Verify `borg start` gives an immediately usable local control surface:
  - health/status
  - session visibility
  - task visibility (when explicit tasks exist)
  - memory search visibility

## 3) Third-party integrations
- Define integration framework (auth/config + health checks + capability registration).
- Add at least one concrete integration path beyond Telegram as a reference pattern.
- Add DB-backed integration configuration records and runtime loading.

## 4) Reintroduce task graph on top of session-first runtime
- Keep Session as the primary interaction entity.
- Reintroduce explicit task graph as agent-managed work:
  - hierarchical tasks (parent/child)
  - dependency/blocking semantics
  - pickup/execution by executor workers
- Formalize ownership model:
  - root session (port conversation)
  - task-owned session lifecycle (start -> progress -> completion)
- Add tests for:
  - task hierarchy creation from agent tools
  - dependency enforcement
  - resumability after restart

## 5) Coordination item
- Schedule a design sync/call to walk through the intended task-graph reintegration model and edge cases before implementing the full subsystem.
