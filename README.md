# agents

`agents` is a Rust toolkit for building typed agent systems and evaluating them.

It includes:

- `agents` for talking to LLMs, managing sessions, tools, context, and storage
- `evals` for defining suites with evaluation trajectories and grading with predicates and judges
- `cargo-evals` for listing and running eval suites
- `codemode` for embeddable JavaScript execution and code search

This README does two things:

1. show the shortest path to your first agent
2. show the shortest path to your first eval

## Getting Started

### Building Agents

To start building agents, add `agents` to your deps:

```bash
cargo add agents
```

Start with a simple string-in, string-out agent (the default):

```rust
use std::sync::Arc;

use agents::{Agent, LlmRunner, SessionAgent};

#[derive(Agent)]
struct BasicAgent(SessionAgent<String, (), (), String>);

impl BasicAgent {
    pub fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
        let agent = SessionAgent::builder()
            .with_llm_runner(llm)
            .build()?;
        Ok(Self(agent))
    }
}
```

And run one turn:

```rust,no_run
# use std::sync::Arc;
# use agents::{Agent, LlmRunner, SessionAgent};
# #[derive(Agent)]
# struct BasicAgent(SessionAgent<String, (), (), String>);
# impl BasicAgent {
#     fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
#         let agent = SessionAgent::builder().with_llm_runner(llm).build()?;
#         Ok(Self(agent))
#     }
# }
# async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
let mut agent = BasicAgent::new(llm)?;
let reply = agent.call("hello world".to_string()).await?;
# let _ = reply;
# Ok(())
# }
```

When you want stricter contracts, switch to typed input and output:

```rust
use std::sync::Arc;

use agents::{Agent, InputItem, LlmRunner, SessionAgent};
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

#[derive(Agent)]
struct TypedAgent(SessionAgent<EchoRequest, (), (), EchoResponse>);

impl TypedAgent {
    fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
        let agent = SessionAgent::builder()
            .with_llm_runner(llm)
            .with_message_type::<EchoRequest>()
            .with_response_type::<EchoResponse>()
            .build()?;
        Ok(Self(agent))
    }
}
```

It is a good idea to make your agents take an `LlmRunner` as a construction
parameter, since the `evals` framework provides one for you to run tests with
many providers and models.

### Evaluating Your Agents

Once you have an agent, you'll want to evaluate how it performs. To do this we'll use the `evals` crate,
which lets us author evals in plain rust code like:

```rust
use agents::SessionAgent;
use anyhow::Result;
use evals::{EvalContext, Trajectory, assistant, eval, predicate, suite, trajectory, user};

type StringyAgent = SessionAgent<String, (), (), String>;

// We set up suites, with a factory for creating the Agent Under Test.
#[suite(
    kind = "regression", 
    agent = new_agent
)]
async fn new_agent(ctx: EvalContext<()>) -> Result<StringyAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(ctx.llm_runner())
        .build()?)
}

// We define trajectories for evaluating interactions with the agent
#[eval(
    agent = StringyAgent, 
    desc = "echoes input", 
    tags = ["smoke"], 
    timeout = "30s"
)]
async fn smoke(_ctx: EvalContext<()>) -> Result<Trajectory<StringyAgent, ()>> {
    Ok(trajectory![
        user!("hello world"),
        assistant!(predicate("echoes-input", |trial, _ctx| async move {
            let reply = trial.final_reply.expect("reply");
            Ok(evals::GradeResult {
                score: if reply == "hello world" { 1.0 } else { 0.0 },
                summary: "agent should echo the input".to_string(),
                evidence: serde_json::json!(reply),
            })
        })),
    ])
}
```

To get started we'll need to install `evals` and do some setup:

```bash
# Install the evals library
cargo add evals
cargo add evals anyhow --build

# Install the `cargo evals` command
cargo install cargo-evals

# Initialize evals in your project
cargo evals init
```

Next we add to a build-step that makes the evals build with `cargo build`:

```rust
// build.rs
fn main() -> anyhow::Result<()> {
    evals::build()?;
    Ok(())
}
```

And then we expose the generated eval registry from your crate root by adding this to `src/lib.rs`:

```rust
evals::setup!();
```

Finally you configure at least one target in your new `evals.toml` file:

```toml
[evals]

[[evals.targets]]
provider = "ollama"
model = "llama3.2:3b"
```

Then run:

```bash
cargo evals list
cargo evals run
```

At that point you have:

- a working agent
- a discovered eval suite
- artifacts under `.evals/`
