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
CRUD, dependencies, review workflow, and queueing.

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
  "assignee": "borg:session:... (required)",
  "reviewer": "borg:session:...|null",
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

### Actor and auth model

TaskGraph does not introduce a separate Actor entity. All mutation/auth/audit
uses `session_uri` (`borg:session:...`), because Borg already models work
identity through sessions.

Rules:

1. Every mutating tool requires `session_uri`.
2. A mutation is allowed only when `session_uri == task.assignee` or
   `session_uri == task.reviewer`.
3. Events always store the acting `session_uri`.
4. `TaskGraph-createTask` is allowed only when `session_uri == assignee` of the
   new task.

### Invariants contributors should rely on

1. The structural graph is always a DAG.
2. A task cannot move to `done` while any child is outside `{done, discarded}`.
3. If `reviewer != null`, `done` requires review approval.
4. `TaskGraph-submitReview` does not set `done`; it marks review intent.
5. `TaskGraph-approveReview` sets `done`.
6. Queue availability requires all blockers to be in `{done, discarded}`.
7. Tasks are always assigned; unassigned tasks are invalid.

### Typical agent flow

1. Create a parent task.
2. Split into explicit subtasks (`TaskGraph-splitTaskIntoSubtasks`).
3. Agents pull available work via `TaskGraph-claimNextWork`.
4. Assignee finishes implementation and calls `TaskGraph-submitReview`.
5. Reviewer approves via `TaskGraph-approveReview`, which sets `done`.
6. Parent becomes completable only when all children are in `{done, discarded}`.

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

## Tool contract

### 1. Task CRUD

1. `TaskGraph-createTask`
2. `TaskGraph-getTask`
3. `TaskGraph-updateTaskFields`

`TaskGraph-updateTaskFields` supports patching:
`title | description | definition_of_done | assignee | reviewer`.
`TaskGraph-createTask` requires non-null `assignee` (session URI).

### 2. Labels

1. `TaskGraph-addTaskLabels`
2. `TaskGraph-removeTaskLabels`

Labels are normalized unique strings prefixed with `#`.

### 3. Parent/child

1. `TaskGraph-setTaskParent`
2. `TaskGraph-clearTaskParent`
3. `TaskGraph-listTaskChildren`

`TaskGraph-setTaskParent` must reject cycles and self-parent.

### 4. Dependencies

Use unambiguous blocked-by API:

1. `TaskGraph-addTaskBlockedBy` (`A` is blocked by `B`)
2. `TaskGraph-removeTaskBlockedBy`

Structural edge direction for cycle checks: `A -> B` when `A` is blocked by `B`.
Derived parent edge for completion gating: `parent -> child`.

### 5. Duplicate/reference links

1. `TaskGraph-setTaskDuplicateOf`
2. `TaskGraph-clearTaskDuplicateOf`
3. `TaskGraph-addTaskReference`
4. `TaskGraph-removeTaskReference`

`references` are non-structural and do not affect availability or DAG checks.
`duplicate_of` is non-structural as an edge, but setting `duplicate_of` has an
operational side-effect: the duplicate task is immediately discarded (with
transitive subtree cascade). Implementation should reject self-duplicate and
duplicate cycles for data hygiene.

### 6. Status transitions

1. `TaskGraph-setTaskStatus`

Rules:

1. cannot set `done` if any child status is outside `{done, discarded}`
2. cannot set `done` if `reviewer != null` and `review.approved_at == null`
3. setting `discarded` MUST atomically and transitively set all descendants to
   `discarded`
4. discard clears review state (`submitted_at`, `approved_at`,
   `changes_requested_at` set to `null`)

### 7. Review flow

1. `TaskGraph-submitReview` (`session_uri` must equal assignee)
2. `TaskGraph-approveReview` (`session_uri` must equal reviewer; sets `status=done`)
3. `TaskGraph-requestReviewChanges` (`session_uri` must equal reviewer; note required)

State effects:

1. submit -> set `submitted_at`, keep `status=doing`
2. approve -> set `approved_at`, clear `changes_requested_at`, set `status=done`
3. request_changes -> set `changes_requested_at`, clear `approved_at`, keep `status=doing`

### 8. Split

1. `TaskGraph-splitTaskIntoSubtasks`

Behavior:

1. rejects if parent is `done`
2. requires each subtask to provide non-null `assignee`
3. sets parent status to `doing`
4. creates `n` child tasks with explicit input fields and no metadata
   inheritance by default (except parent linkage)

### 9. Comments and events

1. `TaskGraph-addComment`
2. `TaskGraph-listComments`
3. `TaskGraph-listEvents`

Events are append-only and server-authored for all mutating calls.

### 10. Queues

1. `TaskGraph-nextWork`
2. `TaskGraph-claimNextWork`
3. `TaskGraph-nextReview`

`TaskGraph-nextWork` and `TaskGraph-claimNextWork` are per-assignee APIs and must
require `session_uri`; there is no global/unfiltered queue mode.

`TaskGraph-nextWork` availability:

1. `status == pending`
2. all `blocked_by` tasks are in `{done, discarded}`
3. if `prefer_leaf_tasks == true`, task has no children
4. task `assignee == session_uri` input

`TaskGraph-claimNextWork`:

1. selects one `next_work` candidate for `session_uri`
2. sets status to `doing` when returning a pending task
3. does not reassign ownership
4. returns `null` when no work is available

`TaskGraph-nextReview` availability:

1. `status == doing`
2. `review.submitted_at != null`
3. `review.approved_at == null`
4. `reviewer == input.session_uri`

### 11. Mutation auth requirement

All mutating tools in sections 1-9 require `session_uri` input and enforce:
`session_uri == assignee || session_uri == reviewer` for the target task.

This includes:

1. `TaskGraph-createTask`, `TaskGraph-updateTaskFields`, `TaskGraph-setTaskParent`, `TaskGraph-clearTaskParent`
2. dependency/duplicate/reference tools
3. `TaskGraph-setTaskStatus`, review tools, split
4. `TaskGraph-addComment`

## Error model

Server errors should map to structured codes:

1. `task.not_found`
2. `task.invalid_uri`
3. `task.validation_failed`
4. `task.dag_cycle_detected`
5. `task.children_incomplete`
6. `auth.session_required`
7. `auth.forbidden`
8. `auth.unknown_session` (optional if caller session must pre-exist)
9. `review.session_mismatch`
10. `review.note_required`

Cycle and invariant violations should return conflict-class semantics (409-like).

## Pagination

List/read APIs with pagination (`TaskGraph-listComments`, `TaskGraph-listEvents`,
queue list calls)
must use:

1. stable ordering: `created_at ASC`, tie-break by ID ascending
2. opaque cursor returned by server
3. explicit `limit` input with max bound `<= 100`

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
Discard-cascade semantics are intentionally opinionated and may be too strong
for teams that expect per-child confirmation before dropping a subtree.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Why this shape:

1. Keep status minimal (`pending|doing|done|discarded`) and encode review via timestamps.
2. Use blocked-by API naming to avoid directional ambiguity.
3. Keep references non-structural to avoid accidental queue starvation.
4. Keep queue logic server-side with assignment-preserving claim semantics.
5. Use session identity (`borg:session:...`) for auth/audit instead of a new
   actor namespace.

Alternatives considered:

1. Add explicit `in_review` status. Rejected to preserve compact status model.
2. Treat `discarded` as not satisfying dependencies. Rejected; discarded now
   counts as complete for dependency/parent gating by policy.
3. Make `references` structural. Rejected because it overloads "see also" links.

## Prior art
[prior-art]: #prior-art

This design draws from issue trackers such as Linear/Jira for assignment and
review workflows, and from build systems/schedulers for strict DAG dependency
resolution and topological availability queues.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Do we want optimistic concurrency fields (e.g., `version`) on mutable writes?
2. Should `auth.unknown_session` be enforced against a session registry, or left
   to caller-side validation in v0?
3. Should `TaskGraph-claimNextWork` always set `doing`, or allow a read-only mode?

## Future possibilities
[future-possibilities]: #future-possibilities

1. SLA fields (`priority`, `due_at`) with queue ordering policies.
2. Batch operations (`TaskGraph-batchUpdateTasks`, `TaskGraph-batchAddTaskBlockedBy`).
3. Cross-project scoping and multi-tenant task namespaces.
4. Streaming subscriptions for queue updates and review inbox changes.
