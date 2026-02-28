# RFD0004 - Apps Expose Capabilities, Internal Tools Power Execution

- Feature Name: `apps_capabilities_execution_runtime`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

Adopt a product model where:

1. **Apps** represent external integrations (uTorrent, SerpAPI, Google Calendar).
2. **Capabilities** represent user-facing actions exposed by an App (`Add Torrent`, `Search Google`, `Create Calendar Event`).
3. **Tools** are no longer user-facing product objects; they are internal runtime primitives (MCP/internal APIs like CodeMode, Shell, Task, Memory, Cron).
4. Capability execution can be:
   - **direct builtin handler**, or
   - **indirect via internal tools** (primarily CodeMode, secondarily Shell).
5. Every invocation is logged in a `tool_calls` table for replay, debugging, and future policy enforcement.

This keeps the UX simple (`App / Capability`) while preserving execution flexibility.

## Motivation
[motivation]: #motivation

Borg needs a clear model that users and operators can reason about quickly.

Current confusion points:

- "Tool" means both product feature and runtime primitive.
- Integrations need account + secret wiring, but execution paths vary.
- We need dynamic coverage of long-tail integrations without building every provider as a first-class builtin.
- We need full execution traces now, before introducing policy engines.

This RFD separates concepts:

- Product surface: `Apps` + `Capabilities`
- Runtime plumbing: internal `Tools`

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

- **App**: External system Borg can connect to.
  - Examples: `uTorrent`, `SerpAPI`, `Google Calendar`.
- **Capability**: Action an App exposes.
  - Examples: `uTorrent / Add Torrent`, `SerpAPI / Search Google`.
- **Internal Tool**: Borg runtime primitive used to execute capability logic.
  - Examples: `CodeMode.runCode`, `Shell.execute`, `Task.createTask`, `Memory.search`.

Users discover and invoke **Capabilities**. Borg decides how to run them.

### Capability discovery and execution flow

1. User asks for intent.
   - Example: "find a legal indie movie torrent and download it".
2. Agent resolves best matching capability.
   - Example result: `SerpAPI / Search Web`, `uTorrent / Add Torrent`, `uTorrent / Get Torrent Status`.
3. Runtime checks capability execution mode.
   - `builtin`: call built-in handler directly.
   - `codemode`: execute generated JS through `CodeMode.runCode`.
   - `shell`: fallback for CLI/system workflows.
4. Runtime returns structured result to agent.
5. Runtime writes invocation records to `tool_calls`.

### Torrent walkthrough

Example target behavior:

1. Use `SerpAPI / Search Web` capability to find a legal `.torrent` or magnet source.
2. Use `uTorrent / Add Torrent` capability to register it.
3. Use `uTorrent / Get Torrent Status` capability to monitor progress.

Possible implementation mapping:

- `SerpAPI / Search Web`: `codemode` execution (`fetch`/SDK call with `SERPAPI_API_KEY`).
- `uTorrent / Add Torrent`: either `builtin` HTTP handler or `codemode` calling local `/gui` endpoint.
- `uTorrent / Get Torrent Status`: same backend choice as above.

User sees only capabilities; runtime may use one or more internal tools.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Data model

#### `apps`

- `app_id` (URI)
- `name` (e.g. `uTorrent`, `SerpAPI`)
- `slug` (stable identifier)
- `description`
- `status` (`active`, `disabled`)
- `created_at`, `updated_at`

#### `app_connections`

Represents account/config connectivity for an app in a user/workspace context.

- `connection_id` (URI)
- `app_id` (FK -> `apps`)
- `user_id` (nullable depending on app scope)
- `workspace_id` (nullable depending on scope)
- `auth_kind` (`oauth`, `api_key`, `local_service`, `none`)
- `auth_ref_json` (secret names/account refs; no raw secret values)
- `config_json` (host/port/default paths/flags)
- `status` (`connected`, `disconnected`, `error`)
- `created_at`, `updated_at`

#### `capabilities`

User-facing operations exposed by apps.

- `capability_id` (URI)
- `app_id` (FK -> `apps`)
- `name` (e.g. `Add Torrent`)
- `slug` (e.g. `add_torrent`)
- `description`
- `input_schema_json` (JSON Schema)
- `output_schema_json` (JSON Schema)
- `execution_mode` (`builtin`, `codemode`, `shell`)
- `execution_spec_json`
  - `builtin`: handler identifier + mapping config
  - `codemode`: prompt/spec template + package hints + env requirements
  - `shell`: command template + sandbox constraints
- `enabled`
- `created_at`, `updated_at`

#### `tool_calls`

Audit log for all internal tool invocations during capability execution.

- `tool_call_id` (URI)
- `session_id`
- `task_id` (nullable)
- `turn_id`
- `app_id` (nullable)
- `capability_id` (nullable)
- `tool_name` (e.g. `CodeMode.runCode`, `uTorrent.addTorrentBuiltin`)
- `invocation_mode` (`builtin`, `codemode`, `shell`)
- `input_json`
- `output_json`
- `status` (`ok`, `error`, `timeout`)
- `error_text` (nullable)
- `started_at`
- `finished_at`
- `duration_ms`

### Internal tools (non-product)

Built-in runtime tools remain first-class for orchestration:

- `CodeMode.*` (package discovery, type/example retrieval, code execution)
- `Shell.*`
- `Cron.*`
- `Task.*`
- `Memory.*`

They are implementation details that capabilities can map to.

### Capability execution contract

Given `(app_id, capability_id, input)`:

1. Validate input against `input_schema_json`.
2. Resolve connection/auth/config from `app_connections` + secrets/account refs.
3. Dispatch by `execution_mode`.
4. Validate output against `output_schema_json` (best effort initial phase).
5. Persist `tool_calls` rows for each internal execution step.
6. Return normalized output to agent.

### CodeMode role

CodeMode is the primary path for long-tail integrations where no dedicated builtin exists.

Expected CodeMode sequence for capability execution:

1. discover/select package(s)
2. inspect docs/types/examples
3. synthesize code from capability spec + input schema
4. execute with scoped env/network/filesystem
5. return structured JSON result

## Drawbacks
[drawbacks]: #drawbacks

- More control-plane entities (`apps`, `capabilities`, `connections`) than a single tool table.
- Requires strong schema discipline for consistent capability behavior.
- Dynamic CodeMode-backed capabilities can be less predictable than dedicated builtins.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Alternative A: Keep "Tools" as user-facing concept

Pros:
- simpler migration from current wording

Cons:
- persistent ambiguity between product tool and runtime tool

Decision: rejected.

### Alternative B: Only builtin integrations

Pros:
- highest control and reliability

Cons:
- low extensibility
- slower shipping for new providers

Decision: rejected.

### Chosen approach

- Product: `Apps expose Capabilities`
- Runtime: internal tool orchestration, mostly CodeMode for long-tail providers
- Observability first via `tool_calls`

## Prior art
[prior-art]: #prior-art

- Integration platforms that expose provider-specific actions under connected apps.
- Capability/action catalogs in workflow automation systems.
- Model-driven code execution using package/docs/type retrieval before runtime execution.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- How strict should output schema validation be in v0 (`warn` vs `hard-fail`)?
- Should `app_connections` be user-scoped, workspace-scoped, or both per app?
- What is the minimum required redaction strategy for `input_json`/`output_json` in `tool_calls`?
- How should capability discovery rank between builtin and CodeMode-backed options when both exist?

## Future possibilities
[future-possibilities]: #future-possibilities

- Capability policy engine (allow/deny/rate limits by user, app, capability).
- Capability composition graphs (multi-capability plans as reusable workflows).
- Promotion pipeline from successful CodeMode executions to stable builtins.
- User-installable external app adapters with the same capability contract.
