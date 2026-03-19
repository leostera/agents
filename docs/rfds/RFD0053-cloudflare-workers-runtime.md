# RFD0053 - Cloudflare Workers Runtime Spike

## Summary

We already support Cloudflare Workers AI as an LLM provider from normal Rust applications. The next question is separate:

- can some part of the `agents` programming model run inside a Cloudflare Worker?

This RFD records the spike boundary and the constraints we need to respect before we try to make that claim.

## Goals

1. Prove a minimal Rust Cloudflare Worker build in this repo.
2. Prove one request handler that can call Workers AI from inside the Worker.
3. Identify the smallest useful subset of the `agents` stack that could be made wasm-safe.

## Non-goals

1. Do not force the existing `agents` crate to become fully wasm-compatible in one pass.
2. Do not redesign the `LlmRunner` provider layer around Workers-specific APIs yet.
3. Do not claim Worker runtime support until we have a real deployable example.

## Why this is a separate problem

Supporting Workers AI as a provider is straightforward:

- the Rust app runs somewhere else
- `agents` talks to Workers AI over its OpenAI-compatible API

Running inside Cloudflare Workers is a different constraint set:

- Rust compiles to `wasm32-unknown-unknown`
- the runtime model is `workers-rs`, not Tokio on a normal host
- native filesystem/process behavior is unavailable
- several existing `agents` dependencies and assumptions are not obviously wasm-safe

## Current state

Today we have:

- `agents` support for `workers_ai` as a provider
- `evals` support for `workers_ai` targets
- an example workspace member:
  - `examples/cloudflare-worker-agent`

That example is intentionally a spike target, not a finished runtime integration.

## Proposed spike sequence

### 1. Minimal Worker proof

Turn `examples/cloudflare-worker-agent` into a real `workers-rs` example that can:

- compile to `wasm32-unknown-unknown`
- answer an HTTP request

No `agents` integration yet.

### 2. Worker-to-Workers-AI proof

From inside the Worker, prove one inference call using one of:

- Workers AI bindings (`env.AI`)
- or the Workers AI OpenAI-compatible REST endpoint

The binding path is probably the better product story, but the REST path may be easier to align with our existing provider model.

### 3. Find the reusable core

Audit which parts of `agents` are plausibly wasm-safe:

- request/response types
- typed tool definitions
- response decoding
- maybe a slimmer provider abstraction

Likely non-starters for a first pass:

- native provider stacks tied to Tokio assumptions
- Apple-specific code
- local filesystem/process tooling

### 4. Decide the product shape

After the spike, pick one:

1. make a wasm-safe subset inside `agents`
2. add a dedicated Worker adapter crate
3. keep Worker support as a separate example only

## Recommendation

Do not try to make `agents` itself Worker-ready in one pass.

The better sequence is:

1. keep Workers AI provider support in `agents`
2. use `examples/cloudflare-worker-agent` as the runtime spike
3. only after that, decide whether a proper `agents-workers` adapter or wasm-safe core split is justified

## References

- Cloudflare Workers AI bindings:
  - <https://developers.cloudflare.com/workers-ai/configuration/bindings/>
- Rust Worker template:
  - <https://github.com/cloudflare/rustwasm-worker-template>
- `workers-rs`:
  - <https://github.com/cloudflare/workers-rs>
