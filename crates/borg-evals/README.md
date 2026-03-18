# borg-evals

`borg-evals` provides typed eval suites, trajectories, grading, and run artifacts for Rust agents.

Core pieces:

- `Suite` and `Eval`
- `Trajectory` and `trajectory![...]`
- deterministic `predicate(...)` graders
- built-in `judge(...)` graders
- registry generation for `cargo evals`

## Example

```rust
use borg_evals::{EvalContext, Trajectory, eval, predicate, suite, trajectory};
use borg_agent::SessionAgent;
use anyhow::Result;

type BasicAgent = SessionAgent<String, (), (), String>;

#[suite(kind = "regression", agent = new_agent)]
async fn new_agent(ctx: EvalContext<()>) -> Result<BasicAgent> {
    Ok(SessionAgent::builder()
        .with_llm_runner(ctx.llm_runner())
        .build()?)
}

#[eval(agent = BasicAgent, desc = "echoes input", tags = ["smoke"])]
async fn smoke(_ctx: EvalContext<()>) -> Result<Trajectory<BasicAgent, ()>> {
    Ok(trajectory![
        user!("hello world"),
        assistant!(predicate("echoes-input", |trial, _ctx| async move {
            let reply = trial.final_reply.expect("reply");
            Ok(borg_evals::GradeResult {
                score: if reply == "hello world" { 1.0 } else { 0.0 },
                summary: "agent should echo the input".to_string(),
                evidence: serde_json::json!({ "reply": reply }),
            })
        })),
    ])
}
```
