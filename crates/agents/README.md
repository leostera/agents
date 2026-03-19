# agents

`agents` is the main crate for building typed agent systems in Rust.

It includes:

- provider-neutral LLM requests and runner APIs
- `SessionAgent` and the `Agent` trait
- typed tools and structured replies
- context and storage integration

## Add it

```bash
cargo add agents
```

## Provider Setup

`LlmRunner` is provider-neutral. Add one or more providers and pick them at request time with `ModelSelector`.

```rust,no_run
use agents::{
    LlmRunner,
    provider::openai::{OpenAI, OpenAIConfig},
};

# fn demo() -> anyhow::Result<()> {
let runner = LlmRunner::builder()
    .add_provider(OpenAI::new(OpenAIConfig::new("sk-...")?))
    .build();
# let _ = runner;
# Ok(())
# }
```

## String agent

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
# async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
let mut agent = SessionAgent::builder().with_llm_runner(llm).build()?;
let reply = agent.call("hello world".to_string()).await?;
# let _ = reply;
# Ok(())
# }
```

Or spawn it and consume events:

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

## Typed agent

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

#[derive(agents::Agent)]
struct EchoAgent {
    #[agent]
    inner: SessionAgent<EchoRequest, (), (), EchoResponse>,
}

impl EchoAgent {
    fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: SessionAgent::builder()
                .with_llm_runner(llm)
                .with_message_type::<EchoRequest>()
                .with_response_type::<EchoResponse>()
                .build()?,
        })
    }
}
```

Run one turn directly:

```rust,no_run
# use std::sync::Arc;
# use agents::{Agent, InputItem, LlmRunner, SessionAgent};
# use schemars::JsonSchema;
# use serde::{Deserialize, Serialize};
# #[derive(Clone, Serialize, Deserialize)]
# struct EchoRequest { text: String }
# impl From<EchoRequest> for InputItem {
#     fn from(value: EchoRequest) -> Self { InputItem::user_text(value.text) }
# }
# #[derive(Clone, Serialize, Deserialize, JsonSchema)]
# struct EchoResponse { text: String }
# #[derive(agents::Agent)]
# struct EchoAgent {
#     #[agent]
#     inner: SessionAgent<EchoRequest, (), (), EchoResponse>,
# }
# impl EchoAgent {
#     fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
#         Ok(Self {
#             inner: SessionAgent::builder()
#                 .with_llm_runner(llm)
#                 .with_message_type::<EchoRequest>()
#                 .with_response_type::<EchoResponse>()
#                 .build()?,
#         })
#     }
# }
# async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
let mut agent = EchoAgent::new(llm)?;
let reply = agent.call(EchoRequest {
    text: "hello".to_string(),
}).await?;
assert_eq!(reply.text, "hello");
# Ok(())
# }
```

## Related crates

- use `evals` for suites, trajectories, predicates, and judges
- use `agents-proc-macros` directly only if you want the proc macro crate itself
- use `agents-test` for provider-specific test helpers
