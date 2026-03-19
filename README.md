# agents

`agents` is a Rust toolkit for building typed agent systems and evaluating them.

The public crates are:

- `agents` for LLMs, sessions, tools, context, and storage
- `evals` for suites, trajectories, grading, judges, and artifacts
- `cargo-evals` for listing and running eval suites
- `codemode` for embeddable JavaScript execution and code search

## What This Repo Gives You

- a typed `LlmRunner` with multiple providers
- a built-in `SessionAgent<M, C, T, R>`
- eval suites and artifacts that live next to your crate
- a Cargo subcommand for listing and running evals
- an embeddable codemode engine for code tools

## Install

```toml
[dependencies]
agents = "0.0.1"
evals = "0.0.1"
anyhow = "1"

[build-dependencies]
evals = "0.0.1"
anyhow = "1"
```

Install the CLI with:

```bash
cargo install cargo-evals
```

## Build an agent

For simple cases, use `String` input and output:

```rust
use std::sync::Arc;

use agents::{LlmRunner, SessionAgent};

type BasicAgent = SessionAgent<String, (), (), String>;

fn new_agent(llm: Arc<LlmRunner>) -> anyhow::Result<BasicAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(llm)
        .build()?)
}
```

Run one turn directly:

```rust,no_run
# use std::sync::Arc;
# use agents::{LlmRunner, SessionAgent};
# type BasicAgent = SessionAgent<String, (), (), String>;
# async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
# let mut agent = SessionAgent::builder().with_llm_runner(llm).build()?;
let reply = agent.call("hello world".to_string()).await?;
# let _ = reply;
# Ok(())
# }
```

Or spawn it and consume the event stream:

```rust,no_run
# use std::sync::Arc;
use agents::{AgentInput, LlmRunner, SessionAgent};

# async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
let agent = SessionAgent::builder().with_llm_runner(llm).build()?;
let (tx, mut rx) = agent.spawn().await?;
tx.send(AgentInput::Message("hello world".to_string())).await?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
# Ok(())
# }
```

For stricter contracts, use typed input and output:

```rust
use std::sync::Arc;

use agents::{InputItem, LlmRunner, SessionAgent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EchoRequest {
    text: String,
}

impl From<EchoRequest> for InputItem {
    fn from(value: EchoRequest) -> Self {
        InputItem::user_text(value.text)
    }
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

## Evaluate it

Register eval discovery in `build.rs`:

```rust
fn main() -> anyhow::Result<()> {
    evals::build()?;
    Ok(())
}
```

Expose the generated registry in `src/lib.rs`:

```rust
evals::setup!();
```

Then define suites under `evals/**/*.rs`:

```rust
use agents::SessionAgent;
use anyhow::Result;
use evals::{
    EvalContext, GradeResult, Trajectory, assistant, eval, predicate, suite, trajectory, user,
};

type BasicAgent = SessionAgent<String, (), (), String>;

#[suite(kind = "regression", agent = new_agent)]
async fn new_agent(ctx: EvalContext<()>) -> Result<BasicAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(ctx.llm_runner())
        .build()?)
}

#[eval(agent = BasicAgent, desc = "echoes the input", tags = ["smoke"], timeout = "30s")]
async fn smoke(_ctx: EvalContext<()>) -> Result<Trajectory<BasicAgent, ()>> {
    Ok(trajectory![
        user!("hello world"),
        assistant!(predicate("echoes-input", |trial, _ctx| async move {
            let reply = trial.final_reply.expect("reply");
            Ok(GradeResult {
                score: if reply == "hello world" { 1.0 } else { 0.0 },
                summary: "agent should echo the input".to_string(),
                evidence: serde_json::json!({ "reply": reply }),
            })
        })),
    ])
}
```

Run them with `cargo-evals`:

```bash
cargo evals list
cargo evals models
cargo evals run
cargo evals --model ollama/llama3.2:3b smoke
```

Artifacts are written under `.evals/`.

## Release Surface

- [`crates/agents`](crates/agents) is the main runtime crate
- [`crates/evals`](crates/evals) is the eval runtime crate
- [`crates/cargo-evals`](crates/cargo-evals) is the CLI
- [`crates/codemode`](crates/codemode) is the embeddable code execution engine
