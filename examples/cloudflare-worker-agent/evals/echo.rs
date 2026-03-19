#[cfg(not(target_arch = "wasm32"))]
use crate::echo::{CloudEchoAgent, CloudEchoRequest, CloudEchoResponse};
#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use evals::{AgentTrial, EvalContext, EvalResult, GradeResult, Trajectory};
#[allow(unused_imports)]
#[cfg(not(target_arch = "wasm32"))]
use evals::{assistant, trajectory, user};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

#[cfg(not(target_arch = "wasm32"))]
#[evals::suite(
    kind = "regression",
    agent = new_agent,
)]
async fn new_agent(ctx: EvalContext<()>) -> Result<CloudEchoAgent> {
    CloudEchoAgent::new(ctx.llm_runner()).await
}

#[cfg(not(target_arch = "wasm32"))]
#[evals::grade(name = "echoes-hello")]
async fn echoes_hello(
    trial: AgentTrial<CloudEchoResponse>,
    _ctx: EvalContext<()>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.text == "hello from the worker example" {
            1.0
        } else {
            0.0
        },
        summary: "cloudflare worker example should preserve the exact input text".to_string(),
        evidence: json!(reply),
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[evals::eval(
    agent = CloudEchoAgent,
    desc = "simple smoke eval for the cloudflare worker example agent",
    tags = ["example", "echo", "cloudflare"],
)]
async fn echoes_plain_text(_ctx: EvalContext<()>) -> Result<Trajectory<CloudEchoAgent, ()>> {
    Ok(trajectory![
        user!(CloudEchoRequest {
            text: "hello from the worker example".to_string()
        }),
        assistant!(echoes_hello()),
    ])
}
