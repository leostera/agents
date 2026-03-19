# TODO

*How to use this file*
Write down here everything you're currently working on, or will work on in the near future
As soon as you are done with something, _remove it from this file_

---

This file tracks the current agent and eval roadmap.

## Current Priority

1. Finish expanding `AgentEvent` into the full event stream
   - request-side coverage is better now
   - keep pushing lifecycle coverage until transcripts can rely on agent events directly
   - likely next areas:
     - queued turn activation / turn boundaries
     - explicit request dispatch failures where they add clarity

2. Finish removing remaining Ollama-local coupling
   - audit remaining tests/examples/defaults for Ollama-specific assumptions that should become generic local-target behavior

## Validation

3. Keep validating external-workspace support
   - `evals`, `evals-macros`, and `cargo-evals` must work from another project, not just this workspace.
   - Continue adding smoke coverage for:
     - external path dependencies
     - setup via `build.rs`
     - `cargo evals list`
     - `cargo evals run`

## Future Work

4. Explore running agents on Cloudflare Workers
   - Workers AI provider support is the easy part
   - the remaining question is how much of the `agents` runtime can run inside the Workers execution model
