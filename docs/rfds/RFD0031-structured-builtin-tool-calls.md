# RFD0031 - Structured Built-in Tool Calls and Results

##Status
Draft

##Summary
Borg should keep built-in tool calls/results structured end-to-end instead of flattening results into prose strings.

This RFD proposes a lightweight, codex-style contract for internal built-in tools:
- tool call: `call_id + name + arguments`
- tool result: `call_id + structured output envelope`

The scope is built-in tools used by Borg actors. This does not redesign MCP wire protocols.
This is a hard cut: no compatibility layer for legacy tool-result payload formats.

##Motivation
Today, Borg has a lightweight call shape but a lossy result path:
1. Model emits structured tool call arguments.
2. Runtime executes the tool and gets typed output.
3. Adapter flattens output into human text (for example `execution result in 37ms: {...}`).
4. Model must parse text back into structure.

This creates unnecessary costs:
- Higher token usage per tool result.
- More parsing ambiguity and retry loops.
- Harder UI rendering and auditing for large payloads.
- Tool authors frequently stringify JSON manually, then adapters stringify again.

##Goals
1. Keep built-in tool results machine-readable through the entire runtime path.
2. Reduce token overhead from prose wrappers.
3. Keep call/result pairing deterministic via `call_id`.
4. Preserve provider compatibility with late fallback to text when required.
5. Improve observability while separating telemetry from model-visible payloads.

##Non-goals
1. Replacing or redesigning MCP transport semantics.
2. Changing actor-to-actor message routing semantics.
3. Rewriting all provider SDKs at once.
4. Preserving legacy tool-result payload compatibility.

##Decisions
1. Built-in tool calls stay minimal and structured.

   ```json
   {
     "call_id": "call_7",
     "name": "Actors:whoAmI",
     "arguments": { "actor_id": "borg:actor:lore" }
   }
   ```

2. Built-in tool results use a canonical structured envelope.

   ```json
   {
     "call_id": "call_7",
     "output": {
       "ok": true,
       "result": { "actor_id": "borg:actor:lore" },
       "error": null,
       "meta": { "duration_ms": 37 }
     }
   }
   ```

3. Adapter fallback rules:
- If provider path supports structured function-call output, send structured output.
- Otherwise send deterministic minified JSON string (no prose wrapper, no English prefixes).

4. Telemetry is out-of-band from model content:
- duration, counters, traces remain runtime metrics.
- model-facing output stays semantic (`ok`, `result`, `error`, `meta`).

5. Interruption/error semantics are explicit:
- interrupted/skipped/denied tool invocations return `ok: false` with machine error payload.
- no synthetic free-text placeholders as the primary representation.

##Before/After Examples
Before (current behavior):

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

After (proposed behavior):

```json
{
  "type": "function_call_output",
  "call_id": "call_7",
  "output": {
    "ok": true,
    "result": { "actor_id": "borg:actor:lore" },
    "meta": { "duration_ms": 37 }
  }
}
```

Before (error):

```json
{
  "type": "tool_result",
  "tool_call_id": "call_9",
  "name": "Actors:receive",
  "content": [{ "type": "text", "text": "tool error: actors.receive.timeout" }]
}
```

After (error):

```json
{
  "type": "function_call_output",
  "call_id": "call_9",
  "output": {
    "ok": false,
    "error": {
      "code": "actors.receive.timeout",
      "message": "timeout waiting for message"
    }
  }
}
```

##Implementation
###1. `borg-agent`: canonical output type
1. Introduce a canonical built-in tool output envelope (`ok`, `result`, `error`, `meta`).
2. Route tool execution records and persisted tool-call logs through this envelope.
3. Keep typed tool authoring helpers, but normalize to structured output before adapter emission.

###2. `borg-agent` adapter boundary
1. Replace prose-oriented tool result flattening with structured conversion.
2. Keep one fallback path for providers requiring text-only tool results:
- encode the same envelope as minified JSON string.

###3. Built-in tool implementations
1. Stop returning JSON-as-string for successful structured responses.
2. Return structured objects directly and let adapter perform final provider-specific shape conversion.
3. Keep human-readable text for truly textual tools (for example explicit human summaries), not for machine payloads.

###4. UI/GraphQL surfaces
1. Keep storing structured tool output payloads.
2. Render tool outputs as JSON by default in Stage/DevMode inspector surfaces.
3. Preserve call/result pairing by `call_id`.

Implementation checklist and file-by-file work breakdown:
- `docs/rfds/RFD0031-implementation-checklist.md`

##Migration and rollout
1. Perform a hard-cut migration for tool result persistence:
- update/drop-recreate tool-call storage schema as needed for canonical structured output.
- do not backfill legacy rows.
2. Remove legacy text-flattening code paths and dual-shape deserialization.
3. Switch to structured built-in tool outputs as default behavior on `main` (no feature flag).
4. If existing local/dev data conflicts with the new schema, truncate/reset affected tables.

##Success metrics
1. Reduce average input tokens per tool-heavy turn.
2. Reduce tool-result parse failures and repair retries.
3. Improve tool-heavy turn latency (`p50/p95`) by reducing unnecessary text churn.
4. Increase share of tool outputs stored as structured payloads.

##Prior art
1. `codex`:
- Uses lightweight call/result pairing (`function_call` + `function_call_output`) and preserves output body structure in protocol/core conversion.

2. `pi-mono`:
- Keeps strong internal tool call/result objects and argument validation.
- For some provider paths, still coerces outputs to text; this demonstrates where complexity grows (normalization/repair logic) when structure is dropped at the boundary.

3. `paperclip`:
- Normalizes tool events for UI (`tool_call`, `tool_result`) but stores result content primarily as string; excellent for transcript UX, weaker for machine semantic replay.

4. `opencode`:
- Rich internal tool lifecycle tracking, but completed tool output is modeled as string in session state and often stringified at provider conversion boundaries.

##Risks
1. Provider-specific tool-result expectations differ; fallback behavior must be tested per provider.
2. Larger structured outputs can increase context size if not truncated intelligently.
3. Existing dev/local history may become unreadable after migration.

##Open questions
1. Should `meta.duration_ms` always be model-visible, or runtime-only by default?
2. Should `error.code` be standardized across all built-in tools in this same RFD or follow-up?
3. Should compacted history keep full structured output or store summarized snapshots for very large results?
