# RFD0001 - Users and Reusable Policies for Session Authorization

- Feature Name: `users_and_reusable_policies`
- Start Date: `2026-02-26`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

Add first-class `User` and `Policy` models to Borg so session turns can be authorized per sender identity, including mixed-permission group chats in Telegram and future multi-user ports.

## Motivation
[motivation]: #motivation

Borg sessions currently operate with an implicit trust model that is too coarse for shared conversations. In a group chat, one participant may be allowed to execute write-capable tools while another should be limited to read-only operations.

Without explicit users and reusable policies:

- Runtime authorization is hard to enforce consistently.
- Prompt-only constraints are non-authoritative.
- Operators cannot safely reuse permission bundles across sessions.

This proposal introduces enforceable, reusable policy primitives so Borg can support real multi-user operation safely.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

Borg introduces these concepts:

- `User`: identity resolved from a port sender (for example Telegram sender id).
- `Policy`: reusable permission bundle (for example `read_only`, `admin`).
- `UserPolicyBinding`: policy attached to one user.
- `SessionPolicyBinding`: policy attached to one session.

Turn flow:

1. A port receives a message and resolves sender metadata into a `User`.
2. Borg computes effective policy for the turn from user and session bindings.
3. Borg injects a compact policy context header so the model can reason correctly.
4. Borg runtime performs authoritative tool-authorization checks before execution.

This keeps behavior explainable to users while guaranteeing enforcement even when model output is wrong.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

Schema additions:

- `users(user_id, external_id, display_name, metadata_json, created_at, updated_at)`
- `policies(policy_id, name, description, metadata_json, created_at, updated_at)`
- `policy_rules(rule_id, policy_id, tool_name, effect, constraints_json, created_at, updated_at)`
- `user_policies(user_id, policy_id, created_at)`
- `session_policies(session_id, policy_id, created_at)`

Optional relationship for richer multi-user sessions:

- `session_users(session_id, user_id, role, created_at)`

Resolution and enforcement rules:

- Effective permissions are derived from `session_policies ∪ user_policies`.
- Rule effect is `allow|deny`, with deny precedence.
- If denied, return structured `permission_denied` and skip tool execution.
- If constrained, validate tool args against policy constraints pre-execution.

Port requirements:

- Ports must provide stable sender identity metadata.
- Telegram minimum metadata: `sender_id`, `sender_username` (optional), `chat_id`, `chat_type`.

Migration:

- Existing sessions start under a permissive compatibility baseline.
- Strict mode can be enabled later to require explicit allow rules.

## Drawbacks
[drawbacks]: #drawbacks

- Adds schema and runtime complexity.
- Requires careful migration to avoid accidental breakage.
- Adds operational overhead for policy lifecycle management.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

- Reusable policy entities are better than ad-hoc per-session flags.
- Prompt-only policy control is insufficient because runtime must enforce security invariants.
- Port-specific hardcoded rules do not scale across Borg ports or deployments.
- Not implementing this keeps Borg unsafe for mixed-trust sessions.

## Prior art
[prior-art]: #prior-art

- IAM/RBAC patterns with explicit deny precedence.
- Policy engines that separate decision from execution.
- Chat bot permission models for group administration.

These provide useful patterns, but Borg should adapt them to its session/task and tool-execution architecture.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- Should constraints support full JSON Schema or a constrained subset?
- What should strict mode defaults be for new deployments?
- How should policy changes propagate into active sessions?

## Future possibilities
[future-possibilities]: #future-possibilities

- Policy simulation endpoint (`why was this denied?`).
- Time-bound and conditional policy bindings.
- Human approval workflows for high-risk actions.
- Admin tooling for policy authoring, review, and rollout.
