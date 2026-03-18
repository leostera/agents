# agents

`agents` is the user-facing Rust crate for this workspace.

It gives you one dependency for:

- `agents::agent` for the runtime
- `agents::llm` for provider integrations and completion types
- `agents::evals` for suites, evals, and runner support

## Add it

```bash
cargo add agents
```

If you want to use eval discovery, also add it to `build-dependencies`.

## Agent example

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

Or spawn the runtime and consume the event stream:

```rust
use agents::agent::AgentInput;

let (tx, mut rx) = agent.spawn().await?;
tx.send(AgentInput::Message("hello world".to_string())).await?;

while let Some(event) = rx.recv().await {
    println!("{event:?}");
}
```

## Typed agent example

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

#[derive(Agent)]
struct TypedAgent(SessionAgent<EchoRequest, (), (), EchoResponse>);

impl TypedAgent {
    fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
        Ok(Self(SessionAgent::builder()
            .with_llm_runner(llm)
            .with_message_type::<EchoRequest>()
            .with_response_type::<EchoResponse>()
            .build()?))
    }
}
```

## Evals example

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
