# Borg TODO

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

## 5) Session search endpoint for Control > Sessions
- Add backend endpoint for searching sessions/messages by free text (not just list/filter).
- Start with SQL-based search on `sessions` + `session_messages` tables to unblock UI.
- Follow up with dedicated `search.db` powered by Tantivy for scalable fuzzy search across sessions/messages.

## 6) Dashboard + Control data coverage
- Providers table should be fully API-backed with real metrics:
  - tokens used
  - token rate
  - models in use
  - cost
  - last used timestamp
  - last session URI
- Add provider-level usage aggregation endpoints instead of deriving from static/placeholder UI data.
- Add session detail enrichment for Control > Sessions:
  - users (already present)
  - providers used (plural)
  - agent ids used
  - message counts / last activity summary

## 7) Memory explorer follow-ups
- Keep `/memory/explorer` search API-driven for large datasets (no client-side entity loading).
- Add explicit explorer query limits/pagination semantics in API responses (for very large graphs).
- Add graph traversal controls in API (depth / edge filters / relationship type filters).

## 8) Observability feature completion
- Implement backend APIs and storage for Observability > Alerts.
- Implement backend APIs for Observability > Tracing (session/task/agent execution traces).
- Define alert/tracing schemas and retention policy.

## 9) Tomorrow
- Reset `sessions` table and ensure all session IDs are valid URIs.
- Continue Agents model migration:
  - make `skills` a dedicated table
  - let agents attach multiple skills/tools
  - for tools, defer final model details until `docs/rfds/RFD0004-connected-accounts-declarative-tools-and-codemode-package-mcp.md` is settled
- Ports UI:
  - show all expected fields
  - make Add Port form support expected fields (including `allows_guests` and `allowed_external_ids`)
- Providers UI:
  - remove placeholder/fake table data
  - clarify provider column naming
  - render real database-backed values directly
