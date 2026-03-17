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

async fn build_agent(_ctx: EvalContext<EchoHarness>) -> Result<EchoAgent> {
    let runner = _ctx.state().runner_for(_ctx.target()).await?;
    EchoAgent::new(runner).await
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "we are testing out a very simple 1-step trajectory",
    tags = ["echo", "baseline"],
)]
async fn echoes_plain_text(
    _ctx: EvalContext<EchoHarness>,
) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(Trajectory::builder()
        .add_step(Step::user(EchoReq("hello".to_string())).expect(
            "echo agent should return the same text",
            GradingConfig::new().grade("echoes-hello", |trial, _ctx| async move {
                let reply: EchoRes = trial.final_reply.unwrap();
                Ok(GradeResult::pass_if(
                    "echoes-hello",
                    reply.text == "hello",
                    "echo agent should preserve the input text",
                    json!({ "reply": reply.text }),
                ))
            }),
        ))
        .build()?)
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "multiline strings are preserved",
    tags = ["echo", "multiline"],
)]
async fn preserves_newlines(
    _ctx: EvalContext<EchoHarness>,
) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(Trajectory::builder()
        .add_step(Step::user(EchoReq("hello\nworld".to_string())).expect(
            "echo agent should preserve newlines",
            GradingConfig::new().grade("echoes-multiline", |trial, _ctx| async move {
                let reply: EchoRes = trial.final_reply.unwrap();
                Ok(GradeResult::pass_if(
                    "echoes-multiline",
                    reply.text == "hello\nworld",
                    "echo agent should preserve multiline input",
                    json!({ "reply": reply.text }),
                ))
            }),
        ))
        .build()?)
}

#[borg_macros::eval(
    agent = EchoAgent,
    desc = "this is a third eval",
    tags = ["echo", "multiline"],
)]
async fn third_eval(_ctx: EvalContext<EchoHarness>) -> Result<Trajectory<EchoAgent, EchoHarness>> {
    Ok(Trajectory::builder()
        .add_step(Step::user(EchoReq("hello\nworld".to_string())).expect(
            "echo agent should preserve newlines",
            GradingConfig::new().grade("echoes-multiline", |trial, _ctx| async move {
                let reply: EchoRes = trial.final_reply.unwrap();
                Ok(GradeResult::pass_if(
                    "echoes-multiline",
                    reply.text == "hello\nworld",
                    "echo agent should preserve multiline input",
                    json!({ "reply": reply.text }),
                ))
            }),
        ))
        .build()?)
}
