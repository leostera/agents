# agents

`agents` is the user-facing Rust crate for this workspace.

It gives you one dependency for:

- `agents::agent` for the agent implementation
- `agents::llm` for provider integrations and completion types
- flat root re-exports for the most common types like `SessionAgent` and `LlmRunner`

## Add it

```bash
cargo add agents
```

If you want evals, add the separate `evals` crate as well.

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

For evals, use the `evals` crate. `agents` no longer re-exports the eval runner surface.
