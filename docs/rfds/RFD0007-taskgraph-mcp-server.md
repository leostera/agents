# RFD0007 - TaskGraph MCP Server for Durable Agent Task DAGs

- Feature Name: `taskgraph_mcp_server`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD defines a new `borg-taskgraph` crate and MCP server, `borg.taskgraph`,
that manages durable tasks as a DAG with Linear-like ergonomics and
agent-friendly queue semantics. The server provides resource URIs for tasks and
append-only logs, enforces acyclic relationships, and exposes a toolset for
CRUD, dependencies, review workflow, queueing, and validation.

## Motivation
[motivation]: #motivation

Borg has internal runtime tasks, but lacks a durable, agent-facing task graph
for planning and execution coordination. We need:

1. a single canonical task model with stable URI IDs (`borg:task:<uuid>`)
2. explicit graph constraints (no cycles, parent completion rules)
3. review-aware completion without introducing extra status values
4. a queue API that lets agents pull "next available work" safely
5. append-only audit and comments for explainability and replay

Without this, agents coordinate through ad-hoc prompts and ephemeral state,
which is brittle under retries, multi-agent execution, and long-running work.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

`borg.taskgraph` stores one primary entity: `Task`.

- "Subtask" is any task with `parent_uri != null`.
- Dependencies are represented as `blocked_by` edges.
- Parent-child implies a derived dependency: child completion gates parent done.
- Comments and events are append-only streams, separate from the task row.

### Canonical task shape

```json
{
  "uri": "borg:task:0f3c2c9e-....",
  "title": "string",
  "description": "string",
  "definition_of_done": "string",
  "status": "pending | doing | done | discarded",
  "assignee": "string|null",
  "reviewer": "string|null",
  "labels": ["#string"],
  "parent_uri": "borg:task:...|null",
  "blocked_by": ["borg:task:..."],
  "duplicate_of": "borg:task:...|null",
  "references": ["borg:task:..."],
  "review": {
    "submitted_at": "RFC3339|null",
    "approved_at": "RFC3339|null",
    "changes_requested_at": "RFC3339|null"
  },
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

### Invariants contributors should rely on

1. The structural graph is always a DAG.
2. A task cannot move to `done` while any child is not `done`.
3. If `reviewer != null`, `done` requires review approval.
4. `review.submit` does not set `done`; it marks review intent.
5. `review.approve` sets `done`.
6. Queue availability requires all blockers to be `done` (strict; `discarded` does not unblock).

### Typical agent flow

1. Create a parent task.
2. Split into explicit subtasks (`task.split_into_subtasks`).
3. Agents pull available work via `queue.claim_next_work`.
4. Assignee finishes implementation and calls `review.submit`.
5. Reviewer approves via `review.approve`, which sets `done`.
6. Parent becomes completable only when all children are `done`.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## Crate and integration boundary

Introduce crate `crates/borg-taskgraph` with:

1. task graph domain model and validators
2. storage adapter (initially SQLite-backed, matching existing Borg DB posture)
3. MCP tool and resource handlers

`borg-cli` remains the only binary crate. `borg-taskgraph` is linked from the
existing runtime/toolchain assembly path similarly to other tool crates.

## Resources

Readable resources:

1. `borg:task:<uuid>` -> full task object
2. `borg:task:<uuid>#comments` -> comment stream (paginated)
3. `borg:task:<uuid>#events` -> event stream (paginated)
4. `borg:task:<uuid>#children` -> direct child list
5. `borg:task:<uuid>#graph` -> bounded local graph snapshot

## Tool contract

### 1. Task CRUD

1. `task.create`
2. `task.get`
3. `task.update_fields`

`task.update_fields` supports patching:
`title | description | definition_of_done | assignee | reviewer`.

### 2. Labels

1. `task.add_labels`
2. `task.remove_labels`

Labels are normalized unique strings prefixed with `#`.

### 3. Parent/child

1. `task.set_parent`
2. `task.clear_parent`
3. `task.list_children`

`task.set_parent` must reject cycles and self-parent.

### 4. Dependencies

Use unambiguous blocked-by API:

1. `task.add_blocked_by` (`A` is blocked by `B`)
2. `task.remove_blocked_by`

Structural edge direction for cycle checks: `A -> B` when `A` is blocked by `B`.
Derived parent edge for completion gating: `parent -> child`.

### 5. Duplicate/reference links

1. `task.set_duplicate_of`
2. `task.clear_duplicate_of`
3. `task.add_reference`
4. `task.remove_reference`

`references` are non-structural and do not affect availability or DAG checks.
`duplicate_of` is non-structural for queueing; implementation should reject
self-duplicate and duplicate cycles for data hygiene.

### 6. Status transitions

1. `task.set_status`

Rules:

1. cannot set `done` if any child status != `done`
2. cannot set `done` if `reviewer != null` and `review.approved_at == null`
3. optional enforcement in v0: `doing` requires non-null assignee

### 7. Review flow

1. `review.submit` (`actor` must equal assignee)
2. `review.approve` (`actor` must equal reviewer; sets `status=done`)
3. `review.request_changes` (`actor` must equal reviewer; note required)

State effects:

1. submit -> set `submitted_at`, keep `status=doing`
2. approve -> set `approved_at`, clear `changes_requested_at`, set `status=done`
3. request_changes -> set `changes_requested_at`, clear `approved_at`, keep `status=doing`

### 8. Split

1. `task.split_into_subtasks`

Creates `n` child tasks with explicit input fields and no metadata inheritance by
default (except parent linkage).

### 9. Comments and events

1. `comment.add`
2. `comment.list`
3. `event.list`

Events are append-only and server-authored for all mutating calls.

### 10. Queues

1. `queue.next_work`
2. `queue.claim_next_work`
3. `queue.next_review`

`queue.next_work` availability:

1. `status == pending`
2. all `blocked_by` tasks are `done`
3. if `prefer_leaf_tasks == true`, task has no children
4. if assignee filter exists, task assignee must match; otherwise unassigned is allowed

`queue.claim_next_work` is atomic:

1. select one `next_work` candidate
2. if unassigned, assign to `actor`
3. set `status=doing`
4. return claimed task or `null`

`queue.next_review` availability:

1. `status == doing`
2. `review.submitted_at != null`
3. `review.approved_at == null`
4. `reviewer == input.reviewer`

### 11. Graph validation

1. `graph.validate`

Returns `{ ok, cycles, violations[] }` over full graph or a rooted subgraph.
This is intended for operational diagnostics and test fixtures.

## Error model

Server errors should map to structured codes:

1. `task.not_found`
2. `task.invalid_uri`
3. `task.validation_failed`
4. `graph.cycle_detected`
5. `graph.parent_incomplete`
6. `review.actor_mismatch`
7. `review.note_required`
8. `queue.claim_conflict`

Cycle and invariant violations should return conflict-class semantics (409-like).

## Storage notes

Minimum durable tables:

1. `taskgraph_tasks`
2. `taskgraph_task_labels`
3. `taskgraph_task_blocked_by`
4. `taskgraph_task_references`
5. `taskgraph_comments`
6. `taskgraph_events`

Writes are transactional per tool call, with event append in the same
transaction as state change.

## Drawbacks
[drawbacks]: #drawbacks

This adds a second "task" concept in Borg (runtime execution tasks vs product
task graph tasks), which can confuse contributors until naming/docs stabilize.
Strict blocker semantics (`discarded` does not unblock) can also feel rigid for
teams that treat discard as explicit cancellation.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Why this shape:

1. Keep status minimal (`pending|doing|done|discarded`) and encode review via timestamps.
2. Use blocked-by API naming to avoid directional ambiguity.
3. Keep references non-structural to avoid accidental queue starvation.
4. Keep queue logic server-side for consistency and atomic claiming.

Alternatives considered:

1. Add explicit `in_review` status. Rejected to preserve compact status model.
2. Treat `discarded` as satisfying dependencies. Rejected for v0 safety.
3. Make `references` structural. Rejected because it overloads "see also" links.

## Prior art
[prior-art]: #prior-art

This design draws from issue trackers such as Linear/Jira for assignment and
review workflows, and from build systems/schedulers for strict DAG dependency
resolution and topological availability queues.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Should `task.set_status(doing)` strictly require `assignee` in v0?
2. Should parent completion allow `discarded` children in a later policy mode?
3. Should `duplicate_of` ever influence queueing (likely no), or remain purely informational?
4. Do we want optimistic concurrency fields (e.g., `version`) on mutable task writes?

## Future possibilities
[future-possibilities]: #future-possibilities

1. SLA fields (`priority`, `due_at`) with queue ordering policies.
2. Batch operations (`task.batch_update`, `task.batch_add_blocked_by`).
3. Cross-project scoping and multi-tenant task namespaces.
4. Streaming subscriptions for queue updates and review inbox changes.
