# RFD0004 - Connected Accounts, Declarative Tools, and CodeMode Package MCP

- Feature Name: `connected_accounts_declarative_tools`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

Introduce a simple integrations model where:

1. Connected Accounts and Secrets provide credentials and OAuth tokens.
2. Tools are declarative records in DB focused on retrieval/planning metadata (name, intent, pseudocode, package hints), not executable source.
3. Execution is always CodeMode JavaScript in a V8 isolate.
4. CodeMode gains an MCP-backed package discovery flow so the LLM can search npm/deno packages, read docs/API signatures, generate code, and execute it.

This keeps tool behavior dynamic and configurable while keeping execution in one runtime path (CodeMode).

## Motivation
[motivation]: #motivation

Borg needs first-class tools that agents can be constrained to, with secure integrations and predictable execution.

Current pain points:

- Integrations need credentials and account linkage, but we do not yet model this cleanly.
- We need dynamic tool definition/update without requiring deploys.
- We need to keep execution safe and debuggable.
- We want strong agent capability boundaries ("this agent can only do scheduling + CRM updates").

If we keep all tools hardcoded, capability management becomes rigid.
If we put full executable JS in rows, we lose too much operational control.

This RFD proposes a simpler middle path: declarative tools in DB + CodeMode execution only.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

- **Connected Accounts**: "Who is connected to which provider?" (Google, Slack, etc).
- **Secrets**: "What static credentials does Borg have?" (OAuth client secret, webhook secret, API keys).
- **Tools**: "What actions exist and how to plan them?" (declarative shape in DB).
- **Execution**: "How actions actually run." (always CodeMode JS in V8).

### Example: Google Calendar

1. Operator configures Google OAuth client in Secrets.
2. User connects Google account in Connected Accounts.
3. Tool definition exists in DB (e.g. `google.calendar.create_event`) with:
   - intent/pseudocode
   - package/API hints
   - required auth references
4. Agent calls tool.
5. Runtime retrieves/refreshes token from Connected Accounts and runs generated JS via CodeMode.

No executable tool JS has to be stored in DB.

### CodeMode + MCP package workflow

For tasks requiring package usage:

1. LLM calls a Package MCP tool (search npm/deno).
2. LLM retrieves docs/API signatures.
3. LLM generates CodeMode snippet.
4. LLM executes snippet in existing V8 isolate runtime.

This keeps package discovery/documentation dynamic while preserving execution under existing sandbox constraints.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Data model

#### `code_locations`

- `location_id` (URI)
- `path` (absolute or BorgDir-relative)
- `enabled`
- `scan_kind` (initial: `local_fs`)
- `created_at`, `updated_at`

`code_locations` declares where Borg should scan for local code-defined tools.

#### `connected_accounts`

- `account_id` (URI)
- `user_id` (URI)
- `provider` (`google`, `github`, etc)
- `external_account_id`
- `scopes_json`
- `token_json_encrypted` (access/refresh/expiry)
- `status` (`active`, `revoked`, `expired`)
- `created_at`, `updated_at`

#### `secrets`

- `secret_id` (URI)
- `namespace` (e.g. `provider.google.oauth.client_secret`)
- `value_encrypted`
- `created_at`, `updated_at`

#### `tools`

- `tool_id` (URI, e.g. `borg:tool:<uuid>`)
- `name` (stable human-readable identifier)
- `description`
- `intent_text` (what the tool is for, when to use it)
- `pseudocode_text` (high-level steps the model should implement)
- `package_hints_json` (npm/deno package names, URLs, preferred SDKs)
- `auth_hints_json` (which connected account/secrets are expected)
- `enabled`
- `created_at`, `updated_at`

#### Agent assignment

Agent specs continue to list allowed tools (by name or tool id).
Runtime resolves those tools from DB and only exposes that set to the model.

### Local tools

In addition to DB-declarative tools, Borg supports local filesystem tools.

A local tool is a directory containing:

- `package.json`
- `tool.js`

`package.json` provides metadata (name, description, version, intent hints, package hints).
`tool.js` provides executable JS entrypoint for the tool.

Discovery flow:

1. Load enabled `code_locations`.
2. Scan directories for valid local tool packages.
3. Validate metadata shape.
4. Register tool in runtime registry (ephemeral) and/or sync into `tools` table as managed entries.

This allows contributors/operators to build tools by writing JS locally, while still using the same runtime policy and agent assignment model.

### Execution contract

Tool execution is unified:

- all tool calls produce a CodeMode JS plan/snippet
- snippet runs in `borg-rt` V8 isolate
- snippet uses Borg SDK + package MCP context as needed
- runtime enforces sandbox/policy/timeouts

### Auth contract

Generated CodeMode snippets call SDK auth primitives:

- `oauth:<provider>` -> token from Connected Accounts (refresh if needed)
- `secret:<namespace>` -> static secret from Secrets

### CodeMode MCP package capability

Add MCP tools callable from CodeMode flow:

- `package.search` (npm + deno)
- `package.getDocs` (README/API examples/signatures)
- `package.getTypes` (when available)

Expected usage chain:

1. discover package
2. inspect docs/types
3. generate code
4. execute code in V8

## Drawbacks
[drawbacks]: #drawbacks

- Adds new control-plane surfaces (accounts, secrets, tool registry).
- Requires careful secret/token encryption and rotation policy.
- Tool quality depends on prompt + metadata quality (intent/pseudocode/package hints).

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Alternative A: Store executable `code_js` in `tools`

Pros:
- maximal flexibility
- no deploy for behavior changes

Cons:
- weaker safety and auditability
- harder compatibility/version management
- easy to create brittle production behavior

Decision: rejected for default path.

### Alternative B: Keep all tools hardcoded

Pros:
- strong safety and maintainability

Cons:
- poor dynamic composition
- slower product iteration

Decision: rejected.

### Chosen approach

Declarative DB tools + unified CodeMode execution gives:

- dynamic capability management
- strong runtime controls
- clear migration path

## Prior art
[prior-art]: #prior-art

- OAuth "connected account" patterns in SaaS integration platforms.
- Declarative API actions in workflow tools (metadata in DB, execution in code).
- MCP-style discovery/docs retrieval for model-guided coding workflows.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- Should agent specs reference tools by `tool_id`, `name`, or both?
- Which encryption backend/key management scheme do we use for `secrets` and token blobs?
- Do we need per-tool rate limits and quotas in first release?
- How strict should package MCP outputs be (only curated packages vs public registries)?

## Future possibilities
[future-possibilities]: #future-possibilities

- Optional "advanced mode" that allows signed/verified `code_js` tools.
- Per-tenant BYO OAuth app credentials.
- Tool execution trace table for full request/response audit.
- Policy-aware tool gating by user/session/port.
