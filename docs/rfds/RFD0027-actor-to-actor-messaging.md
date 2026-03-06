# RFD0027 - Actor-to-Actor Messaging in `borg-exec`

- Feature Name: `actor_to_actor_messaging`
- Start Date: `2026-03-05`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD proposes first-class actor-to-actor messaging inside `borg-exec`.

Core rule: actor-to-actor delivery is not a special lane. It is enqueued and
processed as a normal actor mailbox message using existing `BorgInput::*`
payloads.

v0 is intentionally small:

1. Add `Actors-sendMessage` (durable enqueue, immediate ack).
2. Add `Actors-receive` (wait for a correlated reply by `submission_id`).
3. Define `cast = sendMessage` and `call = sendMessage + receive`.
4. Reuse existing mailbox durability and ordering.
5. No new queue/table/subsystem.

## Motivation
[motivation]: #motivation

We need actors to coordinate internally (planner -> worker, worker -> planner,
system actor -> domain actor) without going through external ports.

At the same time, we do not want a separate priority path that can starve normal
port traffic.

The safest model is to route actor-to-actor messages through the same durable
mailbox path already used by port ingress.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

1. A running actor calls `Actors-sendMessage` with target actor/session and
   `BorgInput` payload.
2. Runtime builds a normal `BorgMessage`, generates a `submission_id`, and
   enqueues it in `actor_mailbox` as `QUEUED`.
3. Target actor processes it in normal mailbox order.
4. If the target sends a reply, that reply carries
   `in_reply_to_submission_id = <original submission_id>`.
5. Caller can use `Actors-receive(expected_submission_id=...)` to wait for that
   correlated reply.

### Message shape

Actor-to-actor messages are represented with existing inputs:

1. `BorgInput::Chat { text }`
2. `BorgInput::Command(BorgCommand::...)`
3. `BorgInput::Audio { ... }` (supported by shape; optional for v0 usage)

No dedicated `ClockworkMessage`/`ActorMessage` payload type is introduced.

### Submission correlation metadata

`submission_id` is transport metadata and lives in the mailbox envelope, not in
`BorgInput`.

This keeps content and transport concerns separated:

1. `BorgInput` remains typed user/task content.
2. Correlation (`submission_id`, `in_reply_to_submission_id`) is mailbox-level.
3. Any `BorgInput` variant can be used with `cast` or `call` without changing
   payload types.

### Ordering and fairness

1. No priority boost for actor-to-actor messages.
2. No bypass around mailbox persistence.
3. Processing order remains mailbox order (`created_at` in `actor_mailbox`).

This keeps actor-to-actor traffic behavior aligned with existing port-to-actor
delivery semantics.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

## Proposed runtime API (v0)

Expose two tools in the default runtime toolchain:

1. `Actors-sendMessage`
2. `Actors-receive`

Operational semantics:

1. `cast = sendMessage`
2. `call = sendMessage + receive(expected_submission_id)`

Request shape (conceptual):

```json
{
  "target_actor_id": "devmode:actor:worker",
  "target_session_id": "borg:session:...",
  "input": {
    "kind": "chat",
    "text": "Please audit docs/rfds/RFD0011-devmode.md."
  }
}
```

Response shape:

```json
{
  "status": "delivered",
  "actor_message_id": "borg:actor_message:...",
  "submission_id": "borg:submission:..."
}
```

`Actors-receive` request shape (conceptual):

```json
{
  "expected_submission_id": "borg:submission:...",
  "timeout_ms": 60000
}
```

`Actors-receive` response shape (conceptual):

```json
{
  "status": "completed",
  "actor_message_id": "borg:actor_message:...",
  "submission_id": "borg:submission:...",
  "in_reply_to_submission_id": "borg:submission:...",
  "source_actor_id": "devmode:actor:worker",
  "source_session_id": "borg:session:...",
  "input": { "kind": "chat", "text": "...." }
}
```

## Runtime behavior

When `Actors-sendMessage` runs:

1. Validate target actor/session ids.
2. Validate target actor exists before enqueue.
3. Build `BorgMessage` with:
   - `actor_id = target_actor_id`
   - `session_id = target_session_id`
   - `input = mapped BorgInput::*`
   - `user_id = current turn user_id` (inherit caller identity)
   - `port_context = Unknown` (v0)
4. Assign a new envelope `submission_id` (`borg:submission:<uuid>`).
5. Enqueue through existing mailbox path and return `status=delivered` plus ids.

When `Actors-receive` runs:

1. Wait for next available reply message for caller actor/session.
2. If `expected_submission_id` is provided, only match messages where
   `in_reply_to_submission_id == expected_submission_id`.
3. Respect `timeout_ms` and return timeout error on deadline.

## Why no `BorgInput::Cast` / `BorgInput::Call`

`BorgInput` should remain content-level input (`Chat`, `Audio`, `Command`).

Call vs cast is transport semantics represented by:

1. `sendMessage` vs `receive` usage.
2. Mailbox correlation metadata (`submission_id`,
   `in_reply_to_submission_id`).

Adding `BorgInput::Cast` / `BorgInput::Call` would mix envelope semantics into
message content and duplicate existing runtime behavior.

## Storage

No new tables in v0.

Existing `actor_mailbox` durability, state transitions, and replay behavior are
reused as-is.

`actor_mailbox` gets two nullable columns:

1. `submission_id`
2. `in_reply_to_submission_id`

Indexing:

1. Index `in_reply_to_submission_id` for `receive(expected_submission_id)` lookups.
2. Keep existing mailbox indexes for normal dequeue order.

## Invariants

1. Delivery boundary is mailbox enqueue.
2. Actor-to-actor messages are never delivered out-of-band.
3. Actor-to-actor messages must not preempt regular ingress.
4. `submission_id` is envelope metadata, never part of `BorgInput`.
5. `call` is always `sendMessage` followed by `receive`.

## Safety constraints for `call` (v0)

1. Reject self-call to same `(actor_id, session_id)` to avoid immediate deadlock.
2. Support per-call timeout (`timeout_ms`), with sane default.
3. Return timeout as tool error; do not block actor loop indefinitely.
4. Optional future: prevent call cycles (`A -> B -> A`) with call-chain metadata.

## Implementation
[implementation]: #implementation

Implementation scope for v0:

1. Add `Actors-sendMessage` and `Actors-receive` tool specs + handlers in
   runtime/admin toolchain.
2. Map typed tool input into existing `BorgInput::*`.
3. Add mailbox correlation fields:
   - `submission_id`
   - `in_reply_to_submission_id`
4. Route sends through existing enqueue/supervisor path.
5. Implement `receive` matching by `expected_submission_id` when provided.
6. Add tests:
   - tool enqueues message for another actor
   - queued message is processed by target actor in normal order
   - `sendMessage` returns `status=delivered` with `submission_id`
   - `receive(expected_submission_id)` returns only correlated reply
   - `call = send + receive` happy path
   - self-call rejection
   - timeout behavior
   - actor-to-actor path does not require any port binding

## Drawbacks
[drawbacks]: #drawbacks

1. `call` introduces risk of deadlocks/cycles if unconstrained.
2. `port_context = Unknown` for actor-origin messages may limit provenance in
   operator views.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

Chosen: reuse existing mailbox path to preserve durability and fairness.

Alternative rejected: separate high-priority internal queue.

Reason: it complicates scheduling/fairness and risks starving normal external
traffic.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Should actor-origin metadata (`source_actor_id`, `source_session_id`) be
   persisted explicitly in mailbox rows or envelope for better observability?
2. Should `Actors-receive` support a stream/progress mode in addition to
   blocking wait in v0?
