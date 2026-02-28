# RFD0004 - Apps Expose Capabilities, Internal Tools Power Execution

- Feature Name: `apps_capabilities_execution_runtime`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD proposes a product model where Apps represent external integrations and Capabilities represent user-facing actions. In this model, integrations such as uTorrent, SerpAPI, and Google Calendar are Apps, while actions like `Add Torrent`, `Search Google`, and `Create Calendar Event` are Capabilities. The term "tool" is reserved for runtime internals such as CodeMode, Shell, Task, Memory, and Cron, and is not used as a user-facing concept. Capability execution may happen through direct builtin handlers or through internal tool chains, with CodeMode as the primary dynamic path and Shell as a fallback. All internal execution steps are recorded in `tool_calls` to support replay, debugging, and later policy enforcement.

This keeps the UX simple (`App / Capability`) while preserving execution flexibility.

## Problem statement

Borg currently mixes two different meanings of "tool":

1. user-facing actions ("send message", "create event", "download torrent"), and
2. internal execution primitives (CodeMode, Shell, Task, Memory, Cron).

That ambiguity makes product design, docs, and runtime contracts harder to reason about. It also slows integration work because we do not have a stable surface for "what users can do" vs "how Borg executes it".

We need a model that is understandable to end users and operators, precise enough for runtime execution and auditing, and extensible enough to support both first-class builtins and long-tail dynamic integrations.

## Motivation
[motivation]: #motivation

Borg needs a clear model that users and operators can reason about quickly.

Today, the word "tool" is overloaded between product features and runtime machinery. Integration wiring also varies by provider because account linkage, secrets, and execution paths are not expressed under a single model. We also need better coverage for long-tail integrations without requiring every provider to become a hardcoded builtin immediately. Finally, we need complete execution traces now, before introducing a policy engine. This RFD addresses those gaps by separating product surface area (`Apps` and `Capabilities`) from runtime plumbing (internal tools).

## Goals and non-goals

### Goals

The goal is to make user-facing concepts explicit and stable by treating Apps and Capabilities as the product language. Execution remains flexible by allowing capabilities to dispatch through `builtin`, `codemode`, or `shell` modes. The proposal also introduces durable invocation logging through `tool_calls` and supports discovery/execution workflows for integrations that are not yet first-class builtins.

### Non-goals (for this RFD)

This document does not define a full policy or authorization framework, does not introduce capability maturity tiers, and does not attempt to ship a full plugin SDK for third-party apps. It also does not attempt to settle every provider-specific contract; it defines the platform model those contracts should fit into.

## Terms

In this document, an **App** is the external integration boundary, and a **Capability** is a concrete operation exposed by that app. An **Internal Tool** is a Borg runtime primitive used to execute capability logic. **Execution mode** refers to how a capability dispatches (`builtin`, `codemode`, or `shell`). The `tool_calls` table stores execution traces for all internal tool invocations.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

An App is an external system Borg can connect to, such as uTorrent, SerpAPI, or Google Calendar. Each App exposes one or more Capabilities, which are the actions users can invoke, such as `uTorrent / Add Torrent` or `SerpAPI / Search Google`. Internal tools like `CodeMode.runCode` and `Shell.execute` are execution machinery, not user-facing product objects. Users discover and invoke capabilities, and Borg decides how to execute them.

### Capability discovery and execution flow

The user starts by expressing intent, for example by asking to find and download a legal indie movie torrent. The agent resolves that request into one or more matching capabilities, such as `SerpAPI / Search Web`, `uTorrent / Add Torrent`, and `uTorrent / Get Torrent Status`. Runtime then dispatches each capability according to its configured execution mode. Builtin capabilities call a dedicated internal handler, codemode capabilities execute generated JavaScript through `CodeMode.runCode`, and shell capabilities use command execution as a fallback path. After execution, runtime returns structured results to the agent and persists invocation records in `tool_calls`.

This creates a clear separation:

- product answers "what can Borg do?" via capabilities,
- runtime answers "how does Borg do it?" via internal tools.

### Torrent walkthrough

Example target behavior:

In a torrent example, the system first calls `SerpAPI / Search Web` to locate a legal `.torrent` or magnet source, then calls `uTorrent / Add Torrent` to register the download, and finally calls `uTorrent / Get Torrent Status` to monitor progress.

Possible implementation mapping:

- `SerpAPI / Search Web`: `codemode` execution (`fetch`/SDK call with `SERPAPI_API_KEY`).
- `uTorrent / Add Torrent`: either `builtin` HTTP handler or `codemode` calling local `/gui` endpoint.
- `uTorrent / Get Torrent Status`: same backend choice as above.

User sees only capabilities; runtime may use one or more internal tools.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Data model

The data model is centered on four tables. The `apps` table stores integration definitions (`app_id`, `name`, `slug`, `description`, `status`, timestamps). The `app_connections` table stores connectivity and auth/config context for an app at user or workspace scope (`connection_id`, `app_id`, optional `user_id` and `workspace_id`, `auth_kind`, `auth_ref_json`, `config_json`, `status`, timestamps). The `capabilities` table stores user-facing operations exposed by apps (`capability_id`, `app_id`, `name`, `slug`, `description`, input/output JSON schemas, `execution_mode`, `execution_spec_json`, `enabled`, timestamps). The `execution_spec_json` payload differs by mode: builtin mode references a handler identifier and mapping config, codemode provides a prompt/spec template plus package and env hints, and shell mode provides command template and sandbox constraints. Finally, `tool_calls` is the execution audit table and captures internal invocation traces (`tool_call_id`, session/task/turn linkage, optional app/capability linkage, tool name, invocation mode, input/output payloads, status/error, timestamps, and duration).

### Internal tools (non-product)

Built-in runtime tools remain first-class for orchestration. In practice this includes the CodeMode family for package discovery, types/examples retrieval, and code execution, along with Shell, Cron, Task, and Memory primitives. These are implementation details that capabilities map to; they are not the product abstraction shown to users.

### Capability execution contract

Given `(app_id, capability_id, input)`:

Given `(app_id, capability_id, input)`, runtime validates input against `input_schema_json`, resolves connection and auth/config context from `app_connections` and secret/account references, dispatches according to `execution_mode`, and then validates output against `output_schema_json` (best-effort in the initial phase). Each internal execution step is persisted in `tool_calls`, and a normalized result is returned to the agent.

### CodeMode role

CodeMode is the primary path for long-tail integrations where no dedicated builtin exists.

For capability execution, CodeMode follows a predictable pattern: discover and select packages, inspect documentation/types/examples, synthesize code from capability spec and input schema, execute with scoped env/network/filesystem permissions, and return a structured JSON result.

## Drawbacks
[drawbacks]: #drawbacks

- More control-plane entities (`apps`, `capabilities`, `connections`) than a single tool table.
- Requires strong schema discipline for consistent capability behavior.
- Dynamic CodeMode-backed capabilities can be less predictable than dedicated builtins.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Alternative A: Keep "Tools" as user-facing concept

This approach offers a simpler migration from current wording, but it preserves ambiguity between product concepts and runtime internals. It is rejected.

### Alternative B: Only builtin integrations

This approach offers maximal control and reliability, but it reduces extensibility and slows delivery of new providers. It is rejected.

### Chosen approach

The chosen direction is to keep `Apps expose Capabilities` as the product model, use internal tool orchestration (primarily CodeMode for long-tail providers) as the runtime model, and prioritize observability through `tool_calls`.

## Prior art
[prior-art]: #prior-art

This proposal draws from integration platforms that expose provider-specific actions under connected apps, from workflow systems that model capability catalogs explicitly, and from model-driven execution loops that retrieve packages/docs/types before generating and running code.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

Open questions remain around output schema strictness in v0 (`warn` versus `hard-fail`), scope rules for `app_connections` (user, workspace, or both), minimum redaction requirements for `tool_calls` payload fields, and ranking behavior when both builtin and CodeMode-backed capability implementations are available.

## Future possibilities
[future-possibilities]: #future-possibilities

Future work can add a capability policy engine, capability composition graphs for reusable workflows, promotion pipelines that turn successful CodeMode executions into stable builtins, and user-installable app adapters that still satisfy the same capability contract.
