use anyhow::Result;
use borg_evals_core::prelude::*;
use serde_json::json;
use crate::echo::{EchoAgent, EchoHarness, EchoReq, EchoRes};

#[borg_evals_macros::suite(
    kind = "regression",
    state = build_harness,
    agent = build_agent,
)]
async fn build_harness() -> Result<EchoHarness> {
    Ok(EchoHarness)
}

async fn build_agent(_ctx: EvalContext<EchoHarness>) -> Result<EchoAgent> {
    EchoAgent::new().await
}

#[borg_evals_macros::eval(
    agent = EchoAgent,
    desc = "we are testing out a very simple 1-step trajectory",
    tags = ["echo", "baseline"],
)]
async fn echoes_plain_text(_ctx: EvalContext<EchoHarness>) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(Trajectory::builder()
        .add_step(
            Step::user(EchoReq("hello".to_string())).expect(
                "echo agent should return the same text",
                GradingConfig::new().grade("echoes-hello", |trial, _ctx| async move {
                    let reply: EchoRes = trial.final_reply.unwrap();
                    Ok(GradeResult::pass_if(
                        "echoes-hello",
                        reply.0 == "hello",
                        "echo agent should preserve the input text",
                        json!({ "reply": reply.0 }),
                    ))
                }),
            ),
        )
        .build()?)
}

#[borg_evals_macros::eval(
    agent = EchoAgent,
    desc = "multiline strings are preserved",
    tags = ["echo", "multiline"],
)]
async fn preserves_newlines(_ctx: EvalContext<EchoHarness>) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(Trajectory::builder()
        .add_step(
            Step::user(EchoReq("hello\nworld".to_string())).expect(
                "echo agent should preserve newlines",
                GradingConfig::new().grade("echoes-multiline", |trial, _ctx| async move {
                    let reply: EchoRes = trial.final_reply.unwrap();
                    Ok(GradeResult::pass_if(
                        "echoes-multiline",
                        reply.0 == "hello\nworld",
                        "echo agent should preserve multiline input",
                        json!({ "reply": reply.0 }),
                    ))
                }),
            ),
        )
        .build()?)
}
