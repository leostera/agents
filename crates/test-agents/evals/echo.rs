use crate::echo::{EchoAgent, EchoHarness, EchoReq, EchoRes};
use anyhow::Result;
use borg_evals::prelude::*;
use serde_json::json;

#[borg_macros::suite(
    kind = "regression",
    state = build_harness,
    agent = build_agent,
)]
async fn build_harness() -> Result<EchoHarness> {
    EchoHarness::new().await
}

async fn build_agent(ctx: EvalContext<EchoHarness>) -> Result<EchoAgent> {
    let runner = ctx.state().runner_for(ctx.target()).await?;
    EchoAgent::new(runner).await
}

#[borg_macros::grade(name = "echoes-hello")]
async fn echoes_hello(
    trial: AgentTrial<EchoRes>,
    _ctx: EvalContext<EchoHarness>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.text == "hello" { 1.0 } else { 0.0 },
        summary: "echo agent should preserve the input text".to_string(),
        evidence: json!({ "reply": reply.text }),
    })
}

#[borg_macros::grade(name = "echoes-multiline")]
async fn echoes_multiline(
    trial: AgentTrial<EchoRes>,
    _ctx: EvalContext<EchoHarness>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.text == "hello\nworld" { 1.0 } else { 0.0 },
        summary: "echo agent should preserve multiline input".to_string(),
        evidence: json!({ "reply": reply.text }),
    })
}

#[borg_macros::grade(name = "echoes-empty")]
async fn echoes_empty(
    trial: AgentTrial<EchoRes>,
    _ctx: EvalContext<EchoHarness>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.text.is_empty() { 1.0 } else { 0.0 },
        summary: "echo agent should preserve empty string".to_string(),
        evidence: json!({ "reply": reply.text }),
    })
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "we are testing out a very simple 1-step trajectory",
    tags = ["echo", "baseline"],
)]
async fn echoes_plain_text(
    _ctx: EvalContext<EchoHarness>,
) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(trajectory![
        user!(EchoReq("hello".to_string())),
        assistant!(echoes_hello()),
    ])
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "multiline strings are preserved",
    tags = ["echo", "multiline"],
)]
async fn preserves_newlines(
    _ctx: EvalContext<EchoHarness>,
) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(trajectory![
        user!(EchoReq("hello\nworld".to_string())),
        assistant!(echoes_multiline()),
    ])
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "empty string is empty string",
    tags = ["echo", "multiline"],
)]
async fn preserves_empty_string(
    _ctx: EvalContext<EchoHarness>,
) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(trajectory![
        user!(EchoReq("".to_string())),
        assistant!(echoes_empty()),
    ])
}
