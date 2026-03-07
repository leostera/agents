# RFD0031 - Structured Built-in Tool Calls and Results

##Status
Draft

##Summary
Borg should keep built-in tool traffic structured end-to-end, instead of flattening tool results into prose strings.

This RFD introduces a hard cut for built-in tools:
1. tool calls remain `call_id + name + arguments`,
2. tool results use a canonical structured envelope (`ok`/`by_design`/`error`),
3. provider adapters only stringify as a final transport fallback, never as runtime semantics.

Scope is built-in tools used by actors. MCP wire format redesign is out of scope.

##Motivation
Today, the runtime frequently converts structured tool outputs into text (for example `execution result in 37ms: {...}`), then models must parse that text back into structure.

This creates avoidable costs:
1. token overhead from prose wrappers,
2. ambiguity and parse failures on retries,
3. brittle UI rendering for non-text payloads,
4. duplicated serialization logic across tools/adapters.

##Goals
1. Preserve machine-readable built-in tool output through runtime and persistence.
2. Remove prose wrappers from tool-result semantics.
3. Keep deterministic call/result pairing by `tool_call_id`.
4. Support provider-specific fallback without changing the canonical runtime shape.
5. Keep migration destructive and simple (no compatibility shim).

##Non-goals
1. Redesigning MCP transport.
2. Changing actor-to-actor routing semantics.
3. Preserving legacy `ToolResultData::Text` behavior.
4. Backfilling old message rows into the new envelope.

##Canonical Contract
### Tool call

```json
{
  "tool_call_id": "call_7",
  "tool_name": "Actors:whoAmI",
  "arguments": { "actor_id": "borg:actor:lore" }
}
```

### Tool result

```json
{
  "tool_name": "Actors:whoAmI",
  "output": {
    "status": "ok",
    "data": {
      "actor_id": "borg:actor:lore"
    }
  }
}
```

Envelope variants:
1. `ok`: successful operation with structured payload.
2. `by_design`: intentional no-op/guardrail result (not an execution failure).
3. `error`: machine-readable error payload/message.

Internal Rust contract:
1. `ToolResponse.output` is canonical runtime storage.
2. `ToolOutputEnvelope::{Ok(T), ByDesign(T), Error(String)}` is the v1 envelope.
3. `Error(String)` is intentionally string-only in v1 (typed `{code,message}` is follow-up).

##Adapter Rules
1. Runtime semantics stay structured until provider boundary.
2. If provider supports structured function outputs, pass envelope as structured JSON payload.
3. If provider requires text tool content, emit minified JSON of the same envelope.
4. Never add English wrappers like `execution result in...` or `tool error:...` as canonical content.

Current implementation note:
1. Provider tool-result content is currently sent as `ProviderBlock::Text(<minified envelope JSON>)`.
2. This preserves structure semantically while remaining compatible with current provider message plumbing.

##Before / After
Before:

```json
{
  "type": "tool_result",
  "tool_call_id": "call_7",
  "name": "Actors:whoAmI",
  "content": [
    {
      "type": "text",
      "text": "execution result in 37ms: {\"actor_id\":\"borg:actor:lore\"}"
    }
  ]
}
```

After (runtime envelope):

```json
{
  "tool_name": "Actors:whoAmI",
  "output": {
    "status": "ok",
    "data": {
      "actor_id": "borg:actor:lore"
    }
  }
}
```

Before (error):

```json
{
  "type": "tool_result",
  "tool_call_id": "call_9",
  "name": "Actors:receive",
  "content": [
    { "type": "text", "text": "tool error: actors.receive.timeout" }
  ]
}
```

After (error envelope):

```json
{
  "tool_name": "Actors:receive",
  "output": {
    "status": "error",
    "data": "actors.receive.timeout"
  }
}
```

After (provider payload fallback):

```json
{
  "type": "tool_result",
  "tool_call_id": "call_9",
  "name": "Actors:receive",
  "content": [
    {
      "type": "text",
      "text": "{\"status\":\"error\",\"data\":\"actors.receive.timeout\"}"
    }
  ]
}
```

##Implementation
###1. Canonical types (`borg-agent`)
1. Replace legacy `ToolResultData::{Text,Capabilities,Execution,Error}` with canonical envelope variants.
2. Keep `ToolRequest` and `tool_call_id` pairing unchanged.
3. Ensure `Message::ToolResult` and `ToolCallRecord` store the new envelope.

###2. Adapter boundary (`borg-agent` + `borg-llm`)
1. Remove prose flattening helpers in `llm_adapter`.
2. Map tool results to deterministic JSON payloads.
3. Keep provider-specific fallback only at serialization boundary.

###3. Built-in tools
1. Stop returning JSON-as-string for structured payloads.
2. Return structured JSON values in `ToolOutputEnvelope::Ok`.
3. Use `ByDesign` for expected no-op outcomes where appropriate.

###4. Persistence and projections
1. Persist structured output payloads in `tool_calls.output_json`.
2. Ensure GraphQL and Stage/DevMode can render structured tool result envelopes.
3. Keep call/result bundling by `tool_call_id`.

##Migration and rollout
1. Hard cut on `main`; no dual-shape runtime support.
2. Remove legacy flattening code paths.
3. Clear local/dev rows that depend on old text flattening as needed.
4. Update actor prompts to rely on structured tool outputs.

##Success metrics
1. Lower average input tokens on tool-heavy turns.
2. Lower tool-result parse/repair failures.
3. Lower p50/p95 turn latency for tool-heavy interactions.
4. Higher share of structured tool outputs persisted in `tool_calls`.

##Prior art
1. Codex: lightweight call/output pairing with structured payload-first semantics.
2. pi-mono: strong internal typed tool objects, with complexity increases when output is coerced to strings.
3. paperclip: good tool event timeline UX, but primarily string-oriented result content.
4. opencode: rich lifecycle tracking, but string output as terminal state in many flows.

##Risks
1. Provider adapters differ on tool-result content expectations.
2. Large structured outputs can bloat context unless bounded.
3. Dev/local data with legacy rows may be unreadable without reset.

##Open questions
1. Do we standardize error payload shape in this RFD (`code` + `message`) or a follow-up?
2. Should `ByDesign` always be model-visible or partially telemetry-only?
3. Do we enforce deterministic JSON key order at adapter boundary?
4. If/when provider adapters support native structured tool-result blocks, do we remove text fallback entirely?
