use crate::echo::{EchoAgent, EchoRequest, EchoResponseFormat};
use agents::prelude::*;
use anyhow::Result;
use serde_json::json;

#[suite(
    kind = "regression",
    agent = new_agent,
)]

async fn new_agent(ctx: EvalContext<()>) -> Result<EchoAgent> {
    EchoAgent::new(ctx.llm_runner()).await
}

#[grade(name = "echoes-hello")]
async fn echoes_hello(
    trial: AgentTrial<EchoResponseFormat>,
    _ctx: EvalContext<()>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.boogyboo == "hello" { 1.0 } else { 0.0 },
        summary: "echo agent should preserve the input text".to_string(),
        evidence: json!(reply),
    })
}

#[grade(name = "echoes-multiline")]
async fn echoes_multiline(
    trial: AgentTrial<EchoResponseFormat>,
    _ctx: EvalContext<()>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.boogyboo == "hello\nworld" {
            1.0
        } else {
            0.0
        },
        summary: "echo agent should preserve multiline input".to_string(),
        evidence: json!(reply),
    })
}

#[grade(name = "echoes-empty")]
async fn echoes_empty(
    trial: AgentTrial<EchoResponseFormat>,
    _ctx: EvalContext<()>,
) -> EvalResult<GradeResult> {
    let reply = trial.final_reply.expect("echo reply");
    Ok(GradeResult {
        score: if reply.boogyboo.is_empty() { 1.0 } else { 0.0 },
        summary: "echo agent should preserve empty string".to_string(),
        evidence: json!(reply),
    })
}

#[eval(
    agent = EchoAgent,
    desc = "we are testing out a very simple 1-step trajectory",
    tags = ["echo", "baseline"],
)]
async fn echoes_plain_text(_ctx: EvalContext<()>) -> Result<Trajectory<EchoAgent, ()>> {
    Ok(trajectory![
        user!(EchoRequest {
            pepo: "hello".to_string()
        }),
        assistant!(GradingConfig::new().grader(echoes_hello()).grader(judge(
            "judge-echoes-hello",
            "Did the assistant preserve the exact input text `hello`?",
        ))),
    ])
}

#[eval(
    agent = EchoAgent,
    desc = "multiline strings are preserved",
    tags = ["echo", "multiline"],
)]
async fn preserves_newlines(_ctx: EvalContext<()>) -> Result<Trajectory<EchoAgent, ()>> {
    Ok(trajectory![
        user!(EchoRequest {
            pepo: "hello\nworld".to_string()
        }),
        assistant!(echoes_multiline()),
        user!(EchoRequest {
            pepo: "hello\nwiht longer lines\nand consecutive newlines\n\n\n".to_string()
        }),
        assistant!(echoes_multiline()),
    ])
}

#[eval(
    agent = EchoAgent,
    desc = "empty string is empty string",
    tags = ["echo", "multiline"],
)]
async fn preserves_empty_string(_ctx: EvalContext<()>) -> Result<Trajectory<EchoAgent, ()>> {
    Ok(trajectory![
        user!(EchoRequest {
            pepo: "".to_string()
        }),
        assistant!(echoes_empty()),
    ])
}
