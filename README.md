# agents

`agents` is a Rust toolkit for building, running, and evaluating typed LLM agents.

There are two user-facing entrypoints:

- `agents` for code
- `cargo-evals` for running eval suites

## What it gives you

- A single `Agent` trait for typed agents
- `SessionAgent` for the default model-backed runtime
- Typed tool calling and structured replies
- A turn-based API for tests and a spawned runtime for long-running systems
- Reusable eval suites with deterministic predicates and LLM judges
- JSON artifacts under `.evals/` for every run

## Quick start

Add the crate:

```
cargo add agents
```

If you want to run evals with `cargo evals`, also add it to your `build-dependencies`.

### Untyped agent

For simple cases, use `String` input and `String` output:

```rust
use std::sync::Arc;

use agents::agent::SessionAgent;
use agents::llm::LlmRunner;

type BasicAgent = SessionAgent<String, (), (), String>;

fn new_agent(llm: Arc<LlmRunner>) -> anyhow::Result<BasicAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(llm)
        .build()?)
}
```

Run one turn directly:

```rust
let reply = agent.call("hello world".to_string()).await?;
```

Or spawn it and drive it through channels:

```rust
use agents::agent::AgentInput;

let (tx, mut rx) = agent.spawn().await?;
tx.send(AgentInput::Message("hello world".to_string())).await?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
```

### Typed agent

For stricter contracts, provide a message type and a response type:

```rust
use std::sync::Arc;

use agents::agent::SessionAgent;
use agents::llm::LlmRunner;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EchoRequest {
    text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct EchoResponse {
    text: String,
}

type TypedAgent = SessionAgent<EchoRequest, (), (), EchoResponse>;

fn new_typed_agent(llm: Arc<LlmRunner>) -> anyhow::Result<TypedAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(llm)
        .with_message_type::<EchoRequest>()
        .with_response_type::<EchoResponse>()
        .build()?)
}
```

Then call it with the typed input:

```rust
let reply = agent
    .call(EchoRequest {
        text: "hello world".to_string(),
    })
    .await?;
```

## Evals Setup

```toml
[dependencies]
agents = "0.0.1"
anyhow = "1"

[build-dependencies]
agents = "0.0.1"
anyhow = "1"
```

Register eval discovery in `build.rs`:

```rust
fn main() -> anyhow::Result<()> {
    agents::evals::build()?;
    Ok(())
}
```

Expose the generated registry in `src/lib.rs`:

```rust
agents::evals::setup!();
```

Then define suites under `evals/**/*.rs`:

```rust
use agents::{
    agent::SessionAgent,
    evals::{EvalContext, Trajectory, eval, suite, trajectory},
};
use anyhow::Result;

type BasicAgent = SessionAgent<String, (), (), String>;

#[suite(kind = "regression", agent = new_agent)]
async fn new_agent(ctx: EvalContext<()>) -> Result<BasicAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(ctx.llm_runner())
        .build()?)
}

#[eval(agent = BasicAgent, desc = "dummy eval", tags = ["smoke"])]
async fn dummy_eval(_ctx: EvalContext<()>) -> Result<Trajectory<BasicAgent, ()>> {
    Ok(trajectory![user!("hello world"),])
}
```

List or run the discovered evals:

```bash
cargo evals list
cargo evals run
cargo evals --model ollama/llama3.2:3b echo
```
