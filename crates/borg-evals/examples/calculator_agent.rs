use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use borg_agent::{
    Agent, AgentResult, ContextManager, ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
    ToolRunner,
};
use borg_evals::prelude::*;
use borg_llm::completion::InputItem;
use borg_llm::runner::LlmRunner;
use borg_llm::testing::{TestContext, TestProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

const DEFAULT_TRIALS: usize = 5;
const DEFAULT_OLLAMA_MODELS: &[(&str, &str)] = &[
    ("llama3.2:1b", "llama3.2:1b"),
    ("llama3.2:3b", "llama3.2:3b"),
    ("mistral-nemo", "mistral-nemo"),
];

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddArgs(u32, u32);

#[derive(Clone, Serialize, Deserialize, JsonSchema, borg_macros::Tool)]
pub enum CalcOp {
    ToolAdd(AddArgs),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum CalcReq {
    Add(u32, u32),
}

impl From<CalcReq> for InputItem {
    fn from(value: CalcReq) -> Self {
        InputItem::user_text(serde_json::to_string(&value).expect("serialize CalcReq as json"))
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalcRes {
    result: u32,
}

#[derive(Clone)]
pub struct CalcToolRunner;

#[async_trait]
impl ToolRunner<CalcOp, u32> for CalcToolRunner {
    async fn run(&self, call: ToolCallEnvelope<CalcOp>) -> AgentResult<ToolResultEnvelope<u32>> {
        match call.call {
            CalcOp::ToolAdd(AddArgs(a, b)) => {
                let result = ToolExecutionResult::Ok { data: a + b };
                Ok(ToolResultEnvelope {
                    call_id: call.call_id,
                    result,
                })
            }
        }
    }
}

#[derive(borg_macros::EvalAgent)]
pub struct CalculatorAgent {
    agent: Agent<CalcReq, CalcOp, u32, CalcRes>,
}

impl CalculatorAgent {
    pub async fn new(runner: LlmRunner) -> Result<Self> {
        let agent = Agent::builder()
            .with_tool_runner(CalcToolRunner)
            .with_llm_runner(runner)
            .with_context_manager(ContextManager::static_text(
                "You are a calculator. Reply with only the final numeric answer.",
            ))
            .with_message_type::<CalcReq>()
            .with_response_type::<CalcRes>()
            .build()?;

        Ok(Self { agent })
    }
}

struct CalculatorHarness {
    ollama: Arc<TestContext>,
}

impl CalculatorHarness {
    async fn runner_for(&self, target: &ExecutionTarget) -> Result<LlmRunner> {
        self.ollama
            .runner_for_model(&target.model)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let ollama = TestContext::shared(TestProvider::Ollama)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let suite = Suite::regression("calculator-agent")
        .trials(DEFAULT_TRIALS)
        .state(CalculatorHarness { ollama })
        .agent(|ctx| async move {
            let runner = ctx
                .state()
                .runner_for(ctx.target())
                .await
                .map_err(|error| EvalError::message(error.to_string()))?;

            CalculatorAgent::new(runner).await
        })
        .eval(
            Eval::new("adds-two-integers")
                .tags(["calculator", "arithmetic", "addition"])
                .grading(
                    GradingConfig::new().grade("returns-4", |trial, _ctx| async move {
                        let reply: CalcRes = trial.final_reply.unwrap();
                        Ok(GradeResult {
                            score: if reply.result == 4 { 1.0 } else { 0.0 },
                            summary: "calculator should answer 2 + 2 with exactly 4".to_string(),
                            evidence: json!({ "reply": reply }),
                        })
                    }),
                )
                .run(
                    Trajectory::builder()
                        .add_step(Step::user(CalcReq::Add(2, 2)))
                        .build()?
                        .runner(),
                ),
        );

    let targets = DEFAULT_OLLAMA_MODELS
        .iter()
        .map(|(label, model)| ExecutionTarget::ollama(*label, *model))
        .collect::<Vec<_>>();

    let report = suite
        .run_with(RunConfig::new(targets))
        .persist_to(".evals")
        .run()
        .await?;

    info!(
        suite = "calculator-agent",
        variants = report.variants.len(),
        "finished calculator eval run"
    );
    println!("{}", report.summary_table());

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "borg_evals=info,borg_llm_test=info".to_string()),
        )
        .with_target(false)
        .compact()
        .try_init();
}
