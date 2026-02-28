# RFD0006 - CodeMode.findPackage for Dynamic Capability Authoring

- Feature Name: `codemode_find_package`
- Start Date: `2026-02-28`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

We propose adding a single new internal tool, `CodeMode.findPackage(query)`, so
Borg can dynamically discover implementation options for a capability without
hardcoding package knowledge in source.

The return shape is intentionally small and task-focused:

`list<{ package, types, examples }>`

This gives the agent enough signal to write working code quickly while avoiding
hallucinated APIs and made-up package usage.

## Problem statement

In `RFD0004`, capabilities are data and the runtime can execute capability
instructions through CodeMode. That's good, but we still have a practical gap:
when a new capability is created, the model still needs to figure out *which
package to use* and *how to use it correctly*.

Today that typically means trying to guess npm/jsr packages, guessing whether
types exist, and guessing code from memory. This is slow and brittle.

For example, if we define a new capability like `uTorrent / Add Torrent`, the
agent should not have to improvise package discovery from scratch every time.
It should be able to ask one tool, get a ranked set of candidates, inspect
actual types, and use real examples.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

`CodeMode.findPackage(query)` is a package scout for code generation. It does
not execute business logic by itself. It only answers: "what package should I
use for this capability, and what code shape is likely to work?"

The response includes:

- `pkg`: package identity (name, registry, version, metadata)
- `types`: resolved TypeScript type definitions (when present)
- `examples`: extracted runnable examples

### Example flow

```text
> me: add a capability to interact with SerpAPI
> agent:
  < tool call(CodeMode.findPackage): "serpapi deno typescript"
  > tool resp(CodeMode.findPackage): [
      {
        pkg: { registry: "npm", name: "serpapi", version: "x.y.z" },
        types: { source: "built-in", files: [ ... ] },
        examples: [ "import ...", "client.search(...)" ]
      },
      ...
    ]

> agent: I'll use npm:serpapi and generate capability instructions from these examples.
```

### Why only one tool?

We intentionally keep this as one tool for now. The goal is to unlock dynamic
capability authoring quickly, not to design a full package-intelligence API
surface. If we split this into many tools now, we add complexity before we
know what the stable interface should be.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

`CodeMode.findPackage(query)` is an orchestrator over three internal stages:

1. search npm and jsr for candidate packages,
2. retrieve type information for each candidate, and
3. extract examples that are useful for generated code.

Even though these stages exist internally, the external contract stays a single
call.

### Tool contract

Input:

```ts
type FindPackageInput = {
  query: string;
  limit?: number; // default 10
};
```

Output:

```ts
type FindPackageResult = Array<{
  package: {
    registry: "npm" | "jsr";
    name: string;
    version?: string;
    description?: string;
    score?: number;
    link?: string;
  };
  types: {
    availability: "built-in" | "definitely-typed" | "jsr-native" | "none";
    sourcePackage?: string; // e.g. "@types/webtorrent"
    files?: Array<{ path: string; content: string }>;
  };
  examples: string[];
}>;
```

### Behavior details

Package discovery should query both npm and jsr. Ranking should prefer direct
name/query matches, active packages, and packages that have available types and
runnable examples.

Type retrieval should attempt built-in declarations first, then fallback to
`@types/*` for npm where relevant. For jsr packages, type information is
expected as native TypeScript exports/definitions.

Example extraction should prefer code blocks that look executable (imports,
constructors, real function calls), and avoid prose-only snippets.

### Storage and tracing

Like other internal tools, `CodeMode.findPackage` executions should emit a
`tool_calls` row with input query and structured output payload. This keeps the
selection process inspectable and debuggable when generated code fails later.

### How this fits RFD0004

This tool is not a new product abstraction. It is an internal runtime tool
used by capability execution and capability authoring flows. In other words,
Apps/Capabilities remain the product model, and `CodeMode.findPackage` is one
of the internals that helps those capabilities stay dynamic.

## Drawbacks
[drawbacks]: #drawbacks

A single orchestrator tool is less transparent than exposing each sub-step
separately. It may also return noisy package candidates in ambiguous queries.
That tradeoff is acceptable for v0 because the main goal is speed and reduced
hallucination during capability authoring.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

One alternative is doing nothing and letting the model free-form package
selection. We reject this because we have already seen how often this leads to
hallucinated APIs and wasted turns.

Another alternative is exposing many fine-grained tools now (`searchPackages`,
`getTypes`, `getExamples`, etc). We reject this for now because it expands the
public interface too early. A single `findPackage` call gives us the same
practical outcome with lower coordination overhead.

## Prior art
[prior-art]: #prior-art

Cloudflare's [Code Mode: give agents an entire API in 1,000 tokens](https://blog.cloudflare.com/code-mode-mcp/)
shows the same design pressure: keep the external interface compact while using
code as the flexible execution path over large API surfaces. Their pattern of a
small tool surface plus dynamic code generation maps directly to why
`CodeMode.findPackage` should be a single orchestrated call.

## Future possibilities
[future-possibilities]: #future-possibilities

If this stabilizes, we can later split internals into dedicated primitives, but
that should happen only when real usage data shows clear need. For now, the
only committed public contract in this area is `CodeMode.findPackage(query)`.
