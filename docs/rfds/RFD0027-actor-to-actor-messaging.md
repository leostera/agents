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
6. No new message columns for v0 correlation.
7. Actor call payload is text-only in v0 (`text` in, `text` out).

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
2. Runtime builds a normal `BorgMessage` and enqueues it in `messages` as
   `QUEUED`.
3. Target actor processes it in normal mailbox order.
4. `submission_id` is the original message id (`messages.message_id`) returned
   by `Actors-sendMessage`.
5. If the target sends a reply, that reply uses existing
   `reply_to_message_id = <original submission_id>`.
5. Caller can use `Actors-receive(expected_submission_id=...)` to wait for that
   correlated reply.

### Message shape

Actor-to-actor call/cast in v0 is text-only:

1. `Actors-sendMessage` accepts `text`.
2. `Actors-receive` returns `text`.
3. Internally this is transported as normal `BorgInput::Chat { text }`.

No dedicated `ClockworkMessage`/`ActorMessage` payload type is introduced.

### Submission correlation

`submission_id` is an API-level alias for the persisted message id.

1. `submission_id = messages.message_id` of the sent message.
2. Correlation uses existing `reply_to_message_id`.
3. `BorgInput` stays content-only.
4. Any `BorgInput` variant can be used with `cast` or `call` without payload
   changes.

### Ordering and fairness

1. No priority boost for actor-to-actor messages.
2. No bypass around mailbox persistence.
3. Processing order remains mailbox order (`created_at`, `message_id` in
   `messages`).

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
  "text": "Please audit docs/rfds/RFD0011-devmode.md."
}
```

Response shape:

```json
{
  "status": "delivered",
  "actor_message_id": "borg:actor_message:...",
  "submission_id": "borg:actor_message:..."
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
  "submission_id": "borg:actor_message:...",
  "in_reply_to_submission_id": "borg:actor_message:...",
  "source_actor_id": "devmode:actor:worker",
  "source_session_id": null,
  "text": "...."
}
```

## Runtime behavior

When `Actors-sendMessage` runs:

1. Validate target actor/session ids.
2. Validate target actor exists before enqueue.
3. Build `BorgMessage` with:
   - `actor_id = target_actor_id`
   - `session_id = target_session_id`
   - `input = BorgInput::Chat { text }`
   - `user_id = current turn user_id` (inherit caller identity)
   - `port_context = Unknown` (v0)
4. Enqueue through existing mailbox path (`messages`) and return
   `status=delivered` plus ids.
5. Return `submission_id = message_id`.

When `Actors-receive` runs:

1. Wait for next available reply message for caller actor/session.
2. If `expected_submission_id` is provided, only match messages where
   `reply_to_message_id == expected_submission_id`.
3. Respect `timeout_ms` and return timeout error on deadline.

## Why no `BorgInput::Cast` / `BorgInput::Call`

`BorgInput` should remain content-level input (`Chat`, `Audio`, `Command`).

Call vs cast is transport semantics represented by:

1. `sendMessage` vs `receive` usage.
2. Mailbox correlation via existing message ids (`message_id`,
   `reply_to_message_id`).

Adding `BorgInput::Cast` / `BorgInput::Call` would mix envelope semantics into
message content and duplicate existing runtime behavior.

## Storage

No new tables in v0.

Existing `messages` durability, state transitions, and replay behavior are
reused as-is.

No new columns are required.

Correlation reuses existing fields:

1. `submission_id` maps to `messages.message_id`.
2. `in_reply_to_submission_id` maps to `messages.reply_to_message_id`.

Optional performance follow-up:

1. Add an index on `(receiver_id, reply_to_message_id, created_at)` if receive
   lookups become hot.

## Invariants

1. Delivery boundary is mailbox enqueue.
2. Actor-to-actor messages are never delivered out-of-band.
3. Actor-to-actor messages must not preempt regular ingress.
4. `submission_id` is message-id correlation, never part of message content.
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
2. Map tool `text` input into `BorgInput::Chat`.
3. Route sends through existing enqueue/supervisor path.
4. Implement `receive` matching by `reply_to_message_id` when
   `expected_submission_id` is provided.
5. Add tests:
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
