# RFD0002 - Port Commands

- Feature Name: `port_commands`
- Start Date: `2026-02-26`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

Add a first-class "port command" concept so ports can intercept command-like user inputs (for example `/compact`) and execute explicit runtime actions before normal LLM turn handling.
Commands should run against port-owned session context snapshots, not ad-hoc per-message parsing.

## Motivation
[motivation]: #motivation

Ports already receive platform-specific command patterns (Telegram slash commands today, potentially others later). Treating these as plain chat text mixes control-plane actions with user conversation and makes behavior inconsistent.

A dedicated port-command layer gives us:

- predictable command behavior across ports
- explicit, testable routing for control actions
- cleaner session transcripts (optional out-of-band handling)

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

Each port can define command matchers and handlers.

Example flow for Telegram:

1. Incoming message is `/compact`.
2. Telegram port resolves it as `PortCommand::Compact`.
3. Runtime executes compact action for the bound session.
4. Port sends confirmation message.
5. No normal agent turn is run for that message.

Unknown commands fall back to standard message processing.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

Proposed primitives:

- `PortCommand` enum (initially minimal, e.g. `Compact`, `Help`)
- `PortCommandRouter` trait per port implementation
- `ExecEngine::handle_port_command(...)` for authoritative execution
- `port_session_ctx` persistence (`port + session_id -> ctx_json`) with port-specific codecs (`PortContext`)

Initial command contract:

- command handlers are asynchronous
- handlers receive `port`, `session_id`, sender metadata
- handlers return structured command output (`handled`, optional reply)
- for `/`-prefixed messages, command dispatch is attempted first (unknown commands return command error and do not run a normal agent turn)

Non-goals in first iteration:

- full permission model for commands (covered by policy RFD)
- dynamic user-defined commands
- rich callback button framework

## Drawbacks
[drawbacks]: #drawbacks

- Adds another dispatch path before normal message processing.
- Needs clear tracing/logging so command handling stays debuggable.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- Should command handling emit session messages or only operational events?
- Should unknown slash commands be ignored or surfaced as help?
- Should commands be namespaced by port (`/tg.compact`) internally?

## Future possibilities
[future-possibilities]: #future-possibilities

- Reusable command registry shared across ports.
- Inline button callbacks mapped to port commands.
- Policy-gated command execution by user/session.
