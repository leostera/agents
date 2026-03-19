# cloudflare-worker-agent

This crate is a workspace spike for answering one specific question:

- what would it take to run agent-shaped application code inside a Cloudflare Worker?

It is intentionally separate from `agents` itself. `agents` currently assumes a normal Rust async/runtime environment and is not yet ready to compile cleanly to `wasm32-unknown-unknown`.

## Current status

This crate is now a real `workers-rs` Worker crate:

- it uses the `worker` crate
- it exports a Worker `fetch` handler
- it compiles to `wasm32-unknown-unknown`

It is still a runtime spike, not an `agents` integration.

What we already support:

- `agents` can talk to Workers AI as an LLM provider
- `evals` can target `workers_ai` models

What is still open:

- compiling a useful subset of the runtime to Cloudflare Workers
- deciding whether Worker-side inference should use:
  - Workers AI bindings (`env.AI`)
  - or the OpenAI-compatible Workers AI REST endpoint
- defining the wasm-safe surface area for request handling, tools, and streaming

## Constraints

Cloudflare Workers Rust apps use `workers-rs` and compile to `wasm32-unknown-unknown`.

That has immediate implications for this repo:

- no Tokio runtime in the usual native sense
- no native filesystem/process access
- any dependency we reuse from `agents` must be wasm-compatible
- provider integrations that assume native `reqwest` + Tokio need a separate path or an abstraction seam

## Minimal Worker shape

The likely runtime entrypoint looks like this:

```rust
use worker::*;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    let _ = (req, env);
    Response::ok("hello from a Cloudflare Worker")
}
```

## Next steps

1. Prove one call into Workers AI from inside the Worker.
2. Decide whether the right long-term shape is:
   - a wasm-safe subset of `agents`
   - or a dedicated Worker adapter crate that talks to `agents` concepts from the edge.

## Local validation

```bash
cargo check --target wasm32-unknown-unknown -p cloudflare-worker-agent
```

## References

- Cloudflare Workers AI bindings:
  - <https://developers.cloudflare.com/workers-ai/configuration/bindings/>
- Cloudflare Rust Worker template:
  - <https://github.com/cloudflare/rustwasm-worker-template>
- `workers-rs` repository:
  - <https://github.com/cloudflare/workers-rs>
