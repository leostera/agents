# RFD0011 - DevMode: Trunk-Based Multi-Agent Code Editing with Git Worktrees

- Feature Name: `devmode_trunk_worktrees`
- Start Date: `2026-03-02`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD introduces **DevMode**, a local subsystem that turns Borg into a
multi-actor development runtime for real repository contribution. DevMode runs
many agentic developers in parallel, each inside an isolated Git worktree under
the project root, and integrates work onto `origin/main` through an optimistic
push/rebase retry loop.

DevMode is an extension of Borg's existing actor-based runtime model (already
present in `borg-exec` session runtime). It applies that model to
repository contribution and publish orchestration.

This RFD builds on RFD0009 (Actor Model and Lifecycle) for actor
lifecycle, mailbox semantics, and runtime registry expectations.

The design is explicitly provenance-first. Every meaningful action is captured
in an append-only audit log, and every commit must carry DevMode identity
trailers linking commit history back to actor, Borg agent spec, session, and
message.

## Motivation
[motivation]: #motivation

Borg currently has strong runtime/task primitives, but it does not define a
native operating model for high-volume repository contribution by many agents
working concurrently on one codebase.

The runtime already uses an actor registry pattern for session execution;
this proposal intentionally reuses that
operational model for DevMode contributors, rather than replacing it.

We need a design that:

1. Allows many agents to edit in parallel without stepping on each other.
2. Preserves trunk-based integration instead of long-lived merge workflows.
3. Keeps attribution and auditability strong enough for operational and
   compliance debugging.
4. Uses Git as the source of truth for rollback and content history.
5. Works first on one machine, while remaining compatible with multiple hosts
   coordinating through a shared remote.

Without this, parallel code execution degrades into ad-hoc local scripts,
ambiguous ownership, and weak failure recovery.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

DevMode introduces **actors** as long-lived developer identities:

- `actor_id`: stable DevMode identity (`devmode:actor:<uuid>`)
- `agent_id`: Borg agent spec identity (prompt/tools profile)
- `session_id` and `message_id`: runtime attribution for specific operations

In the actor model from RFD0009, one DevMode actor is long-lived and may own
many sessions over time. Session IDs remain execution attribution, while
`actor_id` is the durable owner identity.
Each DevMode actor uses the same single-mailbox dispatch rule from RFD0009:
resolve `actor_id`, enqueue message, then let actor-internal logic pick session
handling.
For durability, enqueue means persist to `actor_mailbox` first, then notify the
actor through the in-memory channel fast path.

Each actor owns exactly one Git worktree:

`<project_root>/.devmode/worktrees/<actor-id>/`

Each actor also owns a persistent actor branch:

`devmode/<actor-id>`

The actor branch rebases onto `origin/main` when publishing. DevMode never
deletes actor branches.

### Day-to-day flow

1. Agent edits files in its own worktree only.
2. Agent decides when to commit.
3. DevMode commits with required metadata trailers.
4. DevMode publishes to `origin/main`:
   - push
   - if rejected, fetch + rebase + retry
   - on conflict or retry exhaustion, actor is BLOCKED
5. Human/operator unblocks actor when required.

### Required commit trailers

Every actor commit must include:

- `DevMode-Actor-Id: <devmode:actor:...>`
- `DevMode-Agent-Id: <borg-agent-id>`
- `DevMode-Session-Id: <borg-session-id>`
- `DevMode-Message-Id: <borg-message-id>`

### What DevMode does not do in v0.1

- No centralized commit queue.
- No policy gates beyond existing repository Git hooks.
- No periodic background fetch loop.
- No persisted stdout/stderr capture for Git commands.
- No automatic multi-agent conflict escalation.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Goals

DevMode MUST:

1. Support parallel editing via per-actor Git worktrees under the project root.
2. Integrate all work onto `origin/main` with linear history.
3. Rebase onto `origin/main` after push rejection and retry publish.
4. Record state-mutating and mutation-attempt actions in append-only DB audit
   records.
5. Apply edits atomically via whole-file rewrite + atomic replace.
6. Preserve permissions/ownership on rewritten files.
7. Allow commit timing to be agent-driven, with optional amend.

### Non-goals

DevMode does not:

1. Enforce lint/test/format outside repository hooks.
2. Serialize pushes through a global queue.
3. Persist Git stdout/stderr in DB.
4. Run scheduled fetch/sync tasks.
5. Auto-orchestrate collaborative conflict resolution.
6. Implement actor-to-actor communication channels in v0.1.

### Core invariants

1. Worktree isolation: actor file access is constrained to that actor worktree.
2. Worktree location:
   `<project_root>/.devmode/worktrees/<actor-id>/`
3. Trunk integration target is `origin/main`.
4. Branch model:
   - local persistent branch `devmode/<actor-id>`
   - branch rebases onto `origin/main` during publish
   - branch is never deleted by DevMode
5. All commits include required DevMode trailers.
6. Commits are authored by actor identity; pushes use host Git credentials.
7. File edits use atomic replace semantics.
8. All state-changing operations are audit logged.

### Relationship to existing runtime actors

DevMode does not replace the existing session-actor runtime model.

DevMode reuses the same actor/runtime pattern:

1. Existing runtime actors handle conversational/session message processing and
   tool execution.
2. DevMode actors handle repository worktree ownership, commit creation, and
   publish loops.

Both are actor-backed subsystems using the same runtime lifecycle concepts.

### Data model

#### `devmode_projects`

- `id` (uuid/bigint)
- `root_path` (absolute path)
- `created_at`
- `updated_at`

#### `devmode_actors`

- `actor_id` (pk, `devmode:actor:<uuid>`)
- `project_id` (fk)
- `agent_id`
- `status` (`RUNNING | BLOCKED | STOPPED`)
- `blocked_reason` (nullable)
- `blocked_at` (nullable timestamp)
- `created_at`
- `updated_at`

#### `devmode_actions` (append-only)

- `action_id` (uuid)
- `project_id`
- `actor_id`
- `agent_id`
- `session_id`
- `message_id`
- `timestamp_start`
- `timestamp_end` (or `duration_ms`)
- `kind` (`EDIT_FILE`, `GIT_COMMIT`, `GIT_PUSH`, `GIT_FETCH`, `GIT_REBASE`,
  `BLOCK_ACTOR`, etc.)
- `status` (`STARTED | SUCCEEDED | FAILED | SKIPPED`)
- `repo_root`
- `worktree_path`
- `git_head_before` (nullable)
- `git_head_after` (nullable)
- `payload` (json metadata)

### Filesystem layout

For `<root>`:

- `<root>/.git` (trunk clone)
- `<root>/.devmode/worktrees/<actor-id>/` (actor worktree)
- additional local DevMode runtime state may live under `<root>/.devmode/`

### Worktree lifecycle

On DevMode boot:

1. Load projects and actors.
2. Ensure each actor worktree exists and is a valid Git worktree.
3. If missing, create worktree from trunk clone.
4. Ensure branch `devmode/<actor-id>` exists.
5. Ensure actor worktree checks out `devmode/<actor-id>`.
6. Preserve existing worktree state; never auto-clean.

### Editing protocol

Edits are whole-file rewrites with atomic replacement:

1. Read existing target metadata (`mode`, `uid`, `gid`).
2. Write new content to temp file in same directory (same filesystem).
3. Apply original metadata to temp file.
4. Rename temp file over target atomically.

Optional durability hardening (`fsync`) is deferred.

### Commit protocol

DevMode provides commit primitives; agent decides when to call them:

1. Stage (`git add`).
2. Commit (`git commit`, optional `--amend`).
3. Ensure required trailers are present in final message.

If hooks fail, commit fails; agent must remediate or stop.

### Publish protocol

For actor branch `devmode/<actor-id>`:

1. `git push origin HEAD:main`
2. If success, done.
3. If rejected due to non-fast-forward:
   - `git fetch origin main`
   - `git rebase origin/main`
   - if rebase succeeds, retry push
4. If rebase conflicts:
   - preserve conflict state
   - set actor `BLOCKED` with `REBASE_CONFLICT`
   - record failure and blocking actions
5. Retry limit: 5 retries per publish loop on push rejection.
6. On retry exhaustion: set `BLOCKED` with `PUSH_RETRY_EXHAUSTED`.

### Actor state machine

States:

- `RUNNING`
- `BLOCKED`
- `STOPPED`

Initial blocked reasons:

- `REBASE_CONFLICT`
- `PUSH_RETRY_EXHAUSTED`
- `GIT_ERROR`
- `HOOK_FAILURE_LOOP` (optional)

### Crash recovery

On restart:

1. Rehydrate projects/actors.
2. Ensure worktrees and branches.
3. If actor worktree is mid-rebase/conflicted, mark `BLOCKED` with
   `REBASE_CONFLICT`.
4. Otherwise resume actor loop.

### Interface contracts

Suggested internal API surface:

- `EnsureProject(root_path) -> project_id`
- `EnsureActor(actor_id, project_id, agent_id)`
- `EnsureWorktree(project_id, actor_id)`
- `EditFile(actor_id, session_id, message_id, path, new_content)`
- `Commit(actor_id, session_id, message_id, commit_message, allow_amend)`
- `Publish(actor_id, session_id, message_id)`
- `SetActorStatus(actor_id, status, reason?)`

### Security and trust model

1. Pushes use host user credentials.
2. Commits carry actor authorship identity and required DevMode trailers.
3. This preserves host accountability while retaining agent-level provenance.

## Drawbacks
[drawbacks]: #drawbacks

1. High contention on `main` can cause frequent rebase churn.
2. Worktree-per-actor increases local disk usage.
3. Blocking on conflict requires manual intervention in v0.1.
4. Audit completeness increases storage volume and write load.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Chosen approach: optimistic publish with per-actor persistent branches.

Alternatives considered:

1. Global commit/push queue.
   - Rejected for v0.1: simpler conflict profile, but bottlenecks throughput and
     centralizes arbitration complexity.
2. Merge-based integration.
   - Rejected: violates linear-history trunk requirement.
3. Shared branch for all actors.
   - Rejected: incompatible with Git multi-worktree branch checkout constraints.
4. Auto-conflict resolution agents.
   - Deferred: requires robust escalation/review controls not yet defined.

## Prior art
[prior-art]: #prior-art

1. Trunk-based development with short-lived rebased branches.
2. Git worktree usage for parallel local branches in large repositories.
3. Append-only audit/event logging patterns from distributed task systems.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Should `devmode_actions` include a normalized error taxonomy table instead of
   freeform payload classification?
2. Should commit retries (hook failure loops) be capped at the runtime level, or
   left entirely to agent policy?
3. Should actor branch naming escape/sanitize `actor_id` delimiters (`:`), or
   keep verbatim branch names as specified?
4. Should manual unblock be DB-only in v0.1, or require a CLI/API command path?

## Future possibilities
[future-possibilities]: #future-possibilities

1. Automated conflict escalation sessions involving specialized reviewer agents.
2. Multi-host actor scheduling with lease-based ownership for actor loops.
3. Structured multi-file transactional edit primitives.
4. Optional `fsync` durability profile for file replacement.
5. Policy modules for pre-publish validation beyond repository hooks.
