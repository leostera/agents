# RFD0053 - Cloudflare Workers Runtime

- Feature Name: `cloudflare-workers-runtime`
- Start Date: `2026-03-20`
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary
[summary]: #summary

`agents` now supports Cloudflare in two distinct but related ways: it can talk
to Workers AI as an LLM provider from normal Rust applications, and it can run
useful agent-shaped application code inside a Cloudflare Worker through a
`wasm32-unknown-unknown` build and a binding-backed Workers AI transport.

## Motivation
[motivation]: #motivation

This work started from a real product question:

- can we honestly say this framework supports Cloudflare?

That question turned out to hide two different integration problems:

1. supporting **Workers AI** as a model provider
2. supporting **Cloudflare Workers** as an application runtime

Those are connected, but they are not the same.

Supporting Workers AI as a provider from a normal Rust binary is relatively
straightforward:

- the app runs somewhere else
- `agents` talks to Workers AI over HTTP

Running inside a Cloudflare Worker is a different constraint set:

- the target is `wasm32-unknown-unknown`
- the runtime is `workers-rs`
- native process/filesystem assumptions do not hold
- async and transport behavior differ from the native host path

We needed to answer several practical questions:

- can `LlmRunner` support Workers AI without becoming Cloudflare-specific?
- can the same provider support both REST and Worker binding transports?
- can enough of `agents` compile and behave usefully on wasm to make the Worker
  story real?
- what should we claim publicly, and what should we still describe as bounded
  or incomplete?

This RFD records the answer after implementation, not before it.

Specific use cases this work addresses:

- A normal Rust service wants to use Workers AI as just another provider in
  `LlmRunner`.
- A Cloudflare Worker wants to call `agents` application code directly and use
  the `AI` binding instead of shipping account credentials.
- `evals` users want to target Workers AI models the same way they target other
  providers.
- contributors need one clear document explaining what was actually shipped,
  instead of relying on the older spike narrative.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

The right mental model is:

- **Workers AI** is an inference provider.
- **Cloudflare Workers** is an application runtime.

`agents` now supports both sides of that story.

### What we ship today

Today the repo supports all of the following:

- `agents` can talk to Workers AI through `LlmRunner`
- Workers AI can be reached through two transport modes:
  - REST
  - Cloudflare Worker AI binding
- `evals` can target `workers_ai`
- `examples/cloudflare-worker-agent` is a real deployable Worker example
- `agents` builds on both:
  - native targets
  - `wasm32-unknown-unknown`

That means this is no longer just a spike or a sketch. It is a real supported
integration with a deliberately constrained boundary.

### How contributors should think about the Cloudflare support

There is one provider family:

- `provider::cloudflare::*`

Within that family, Workers AI is one provider with two transport backends:

- REST for normal Rust applications
- binding-backed execution for code running inside a Cloudflare Worker

Contributors should not think of the Worker binding as “just the REST API but
closer.” It is its own transport path and deserves its own implementation.

### Example-driven view

For a normal Rust application, the story is:

- build an `LlmRunner`
- add the Workers AI provider with REST config
- make model calls like any other provider-backed runner

For a Cloudflare Worker, the story is:

- compile the crate to `wasm32-unknown-unknown`
- get the `AI` binding from `workers-rs`
- configure the provider for binding mode
- build an `LlmRunner`
- call a real `Agent`

That second story is demonstrated in:

- `examples/cloudflare-worker-agent`

On the wasm target, that example:

- exports a Worker `fetch` handler
- reads JSON requests
- builds an `LlmRunner` from the `AI` binding
- constructs a real `CloudEchoAgent`
- calls the agent
- returns JSON

On native targets, the same example crate:

- uses `agents`
- uses `evals`
- contains a small eval suite

So contributors should think of it as a dual-purpose example:

- an edge deployment example on wasm
- a local agent/eval example on native

### What not to overclaim

This support is real, but it is not total parity.

We should say:

- `agents` supports Workers AI as a provider
- `agents` can run a meaningful subset of runtime behavior inside a Cloudflare
  Worker

We should not say:

- every provider/path behaves identically on native and wasm
- all native streaming behavior exists on wasm
- every `agents` app is automatically Worker-ready

### Impact on maintainability

The most important maintainability outcome is that Cloudflare-specific behavior
is namespaced and transport-specific instead of leaking into the whole runtime.

That keeps the core runtime legible:

- generic `LlmRunner` stays generic
- Cloudflare details stay in `provider::cloudflare::*`
- Worker deployment guidance stays concentrated in the example and this RFD

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Provider architecture

The provider is organized under:

- `crates/agents/src/llm/provider/cloudflare/`

Workers AI itself is split into:

- `workers_ai/mod.rs`
- `workers_ai/workers_ai_rest.rs`
- `workers_ai/workers_ai_binding.rs`

The split is intentional because the transports are genuinely different.

#### REST transport

The REST path is intended for normal Rust applications.

Configuration includes:

- API token
- account id
- default model
- optional base URL override

The provider talks to the Cloudflare Workers AI HTTP API from normal Rust code.

#### Binding transport

The binding path is intended for code running inside a Cloudflare Worker.

Configuration includes:

- default model
- binding name

The actual `Ai` binding is supplied at runtime.

This avoids:

- embedding account credentials in a Worker deployment
- making a needless REST round-trip from a Worker to Cloudflare's own AI
  service

### wasm support changes

Supporting the Worker example required real crate-level work in `agents`.

The shipped state includes:

- Apple-specific code excluded from wasm builds
- provider boundaries that can compile on wasm
- transport behavior adjusted where native assumptions did not hold
- a workable subset of runtime behavior on `wasm32-unknown-unknown`

The result is not “all features on all targets.” The result is:

- native path preserved
- wasm path made real and usable

### Streaming boundary

One of the important technical decisions was not to force exact streaming parity
between native and wasm.

Native hosted-provider streaming is a richer path.

On wasm, the design goal was:

- preserve a coherent `LlmRunner` surface
- allow buffered completion semantics where that is the practical transport
  boundary

That tradeoff is deliberate.

### Worker logging

The Worker example now uses Worker-native logging:

- `worker::console_log!`
- `worker::console_error!`

This replaced earlier `tracing-wasm` experimentation after real runtime issues
in local Worker development.

That is the correct implementation detail for the current example.

### Example runtime shape

`examples/cloudflare-worker-agent` has two target-gated modes:

#### wasm mode

- real Worker `fetch` handler
- JSON request parsing
- binding-backed `WorkersAI` provider setup
- real `CloudEchoAgent` execution
- JSON response

It also includes basic HTTP hygiene:

- `GET /` health/info response
- `GET /favicon.ico` empty response
- `POST /` for actual execution

#### native mode

- normal `agents` usage
- normal `evals` usage
- local eval suite

This makes the example useful for both deployment and local iteration.

## Drawbacks
[drawbacks]: #drawbacks

- The Cloudflare support boundary is real but not uniform across all providers
  and targets.
- The binding-backed path introduces target-specific logic that would not exist
  in a purely HTTP-only provider model.
- The Worker example can create the impression that all `agents` applications
  are Worker-ready when the actual supported subset is narrower.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Why this design

This design keeps the product story coherent:

- one provider-neutral runtime
- one Cloudflare provider family
- transport specialization where it belongs

It also lets us support the best Cloudflare-native path inside Workers without
forcing that transport model onto non-Worker applications.

### Alternatives considered

#### 1. REST-only Workers AI support

We could have supported only the REST transport.

We did not choose that because it is a worse Worker-side story:

- credentials management is worse
- the transport is less idiomatic for the platform
- it ignores the platform-native `AI` binding

#### 2. Separate `agents-workers` crate

We could have introduced a Worker-specific adapter crate immediately.

We did not choose that because the current split is already working:

- generic runtime remains in `agents`
- Cloudflare-specific logic remains namespaced
- the example demonstrates end-to-end deployment

If the Worker-specific surface grows substantially later, this can be revisited.

#### 3. Force full native/wasm parity before claiming support

We could have blocked the integration until every runtime path matched native.

We did not choose that because it would delay a real and useful product surface
for the sake of a false uniformity requirement.

The right standard here is honest support, not artificial symmetry.

## Prior art
[prior-art]: #prior-art

The immediate prior art for this work came from:

- Cloudflare's own Workers AI binding and REST APIs
- `workers-rs` as the Rust runtime integration for Cloudflare Workers

Within this repo, the closest prior art was the earlier runtime spike and the
first REST-only Workers AI provider work. The shipped design diverged from the
initial spike in two important ways:

- the Worker-side path now uses the `AI` binding through `LlmRunner`
- the provider is now explicitly split by transport

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- How much of the native capability surface should eventually be mirrored on
  wasm?
- Which Worker-specific helpers, if any, deserve promotion out of the example
  crate?
- How should we present the wasm/native capability matrix in user-facing docs?

## Future possibilities
[future-possibilities]: #future-possibilities

The most natural next Cloudflare extensions are:

- support for Cloudflare AI Gateway under `provider::cloudflare::*`
- sharper user-facing documentation of the wasm/native capability boundary
- broader Worker-side examples beyond the current echo-style agent

If the Cloudflare-specific runtime surface grows substantially, we can revisit
whether a dedicated adapter crate is justified. Today, the current split is
still the better fit.
