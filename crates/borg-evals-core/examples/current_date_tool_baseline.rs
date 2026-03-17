use std::sync::Arc;

use anyhow::{Context, Result};
use borg_agent::{
    Agent, AgentInput, CallbackToolRunner, ContextManager, ExecutionProfile, ToolCallEnvelope,
    ToolExecutionResult, ToolResultEnvelope,
};
use borg_evals_core::prelude::*;
use borg_llm::completion::{InputItem, ModelSelector, Temperature, TokenLimit};
use borg_llm::error::{Error as LlmError, LlmResult};
use borg_llm::runner::LlmRunner;
use borg_llm::testing::TestContext;
use borg_llm::testing::TestProvider;
use borg_llm::tools::{RawToolDefinition, TypedTool};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

const DEFAULT_TRIALS: usize = 10;
const EXPECTED_CURRENT_DATE: &str = "2026-03-17T12:00:00Z";
const DEFAULT_OLLAMA_MODELS: &[(&str, &str)] = &[
    ("llama3.2:1b", "llama3.2:1b"),
    ("llama3.2:3b", "llama3.2:3b"),
    ("qwen3.5:0.8b", "qwen3.5:0.8b"),
    ("mistral-nemo", "mistral-nemo"),
];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
enum DateTools {
    GetCurrentDate,
}

impl TypedTool for DateTools {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        vec![RawToolDefinition::function(
            "getCurrentDate",
            Some(
                "Return the current UTC datetime as JSON {\"date\":\"2026-03-17T12:00:00Z\"}. Call with an empty object.",
            ),
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        )]
    }

    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
        match name {
            "getCurrentDate" => {
                ensure_object_like(name, arguments)?;
                Ok(Self::GetCurrentDate)
            }
            other => Err(LlmError::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentDateToolResult {
    date: String,
}

#[derive(Debug, Deserialize)]
struct DateResponse {
    date: DateTime<Utc>,
}

struct DateHarness {
    ollama: Arc<TestContext>,
}

impl DateHarness {
    fn new(ollama: Arc<TestContext>) -> Self {
        Self { ollama }
    }

    async fn runner_for(&self, target: &ExecutionTarget) -> Result<LlmRunner> {
        self.ollama
            .runner_for_model(&target.model)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

fn current_date_tool_suite(harness: Arc<DateHarness>) -> Suite {
    Suite::new("current-date-tool-baseline")
        .kind(SuiteKind::Capability)
        .eval(
            Eval::new("returns_current_date_via_tool")
                .tags(["baseline", "tools", "date"])
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                            run_current_date_trial(
                                harness,
                                ctx.target().clone(),
                                "What is the current date and time in UTC? Call getCurrentDate exactly once. Then reply with ONLY a JSON object like {\"date\":\"2026-03-17T12:00:00Z\"} and nothing else.",
                            )
                            .await
                        }
                    }
                })
                .grade(grade("calls_date_tool_once", |trial| async move {
                    let call_count = trial
                        .tool_trace
                        .iter()
                        .filter(|tool| tool.name == "getCurrentDate")
                        .count();
                    Ok(GradeResult::pass_if(
                        "calls_date_tool_once",
                        call_count == 1,
                        "agent should call getCurrentDate exactly once",
                        json!({ "call_count": call_count }),
                    ))
                }))
                .grade(grade("returns_parseable_date_payload", |trial| async move {
                    let reply = trial.final_reply.as_deref().unwrap_or("").trim();
                    let parsed: Result<DateResponse, _> = serde_json::from_str(reply);
                    Ok(GradeResult::pass_if(
                        "returns_parseable_date_payload",
                        parsed.is_ok(),
                        "agent should reply with a JSON payload that parses into Response { date: DateTime<Utc> }",
                        json!({
                            "reply": reply,
                            "expected": { "date": EXPECTED_CURRENT_DATE },
                            "parsed_ok": parsed.is_ok(),
                        }),
                    ))
                }))
                .grade(grade("returns_tool_date", |trial| async move {
                    let reply = trial.final_reply.as_deref().unwrap_or("").trim();
                    let parsed: Result<DateResponse, _> = serde_json::from_str(reply);
                    let matches = parsed
                        .as_ref()
                        .map(|response| response.date.to_rfc3339() == EXPECTED_CURRENT_DATE)
                        .unwrap_or(false);
                    Ok(GradeResult::pass_if(
                        "returns_tool_date",
                        matches,
                        "agent should return the same date that the tool produced",
                        json!({
                            "reply": reply,
                            "expected": EXPECTED_CURRENT_DATE,
                            "parsed_date": parsed.ok().map(|response| response.date.to_rfc3339()),
                        }),
                    ))
                })),
        )
}

async fn run_current_date_trial(
    harness: Arc<DateHarness>,
    target: ExecutionTarget,
    instruction: &str,
) -> EvalResult<AgentTrial> {
    let runner = harness
        .runner_for(&target)
        .await
        .map_err(|error| EvalError::message(error.to_string()))?;
    let tool_runner = build_date_tool_runner();
    let context = ContextManager::static_text(
        "You are a careful assistant. Always call getCurrentDate before answering date questions. Do not guess the date.",
    );

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .with_context_manager(context)
        .with_execution_profile(target_profile(&target))
        .with_tool_runner(tool_runner)
        .build()
        .map_err(|error| EvalError::message(format!("build date agent: {error}")))?;

    let (tx, mut rx) = agent
        .run()
        .await
        .map_err(|error| EvalError::message(format!("start date agent: {error}")))?;
    tx.send(AgentInput::Message(InputItem::user_text(instruction)))
        .await
        .map_err(|error| EvalError::message(format!("send date instruction: {error}")))?;
    drop(tx);

    let mut recorder = AgentTrialRecorder::default();

    while let Some(event) = rx.recv().await {
        match event {
            Ok(event) => recorder.record(&event),
            Err(error) => {
                let partial_trial = recorder.snapshot(json!({
                    "target": target,
                    "expected_date": EXPECTED_CURRENT_DATE,
                }));
                return Err(EvalError::message_with_trial(
                    error.to_string(),
                    partial_trial,
                ));
            }
        }
    }

    if recorder.final_reply().is_none() {
        return Err(EvalError::message_with_trial(
            "agent finished without a final reply",
            recorder.snapshot(json!({
                "target": target,
                "expected_date": EXPECTED_CURRENT_DATE,
            })),
        ));
    }

    Ok(recorder.into_trial(json!({
        "target": target,
        "expected_date": EXPECTED_CURRENT_DATE,
    })))
}

fn build_date_tool_runner() -> CallbackToolRunner<DateTools, CurrentDateToolResult> {
    CallbackToolRunner::new(move |call: ToolCallEnvelope<DateTools>| async move {
        let result = match call.call {
            DateTools::GetCurrentDate => CurrentDateToolResult {
                date: EXPECTED_CURRENT_DATE.to_string(),
            },
        };

        Ok(ToolResultEnvelope {
            call_id: call.call_id,
            result: ToolExecutionResult::Ok { data: result },
        })
    })
}

fn target_profile(target: &ExecutionTarget) -> ExecutionProfile {
    ExecutionProfile {
        model_selector: ModelSelector::from_model(target.model.clone()),
        temperature: Temperature::Value(0.0),
        token_limit: TokenLimit::Max(128),
        ..ExecutionProfile::default()
    }
}

fn ensure_object_like(name: &str, arguments: serde_json::Value) -> LlmResult<()> {
    match arguments {
        serde_json::Value::Object(_) => Ok(()),
        serde_json::Value::Null => Ok(()),
        other => Err(LlmError::InvalidResponse {
            reason: format!("expected object-like arguments for {name}, got {other}"),
        }),
    }
}

fn default_trials() -> usize {
    std::env::var("BORG_EVALS_TRIALS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TRIALS)
}

fn default_targets() -> Vec<ExecutionTarget> {
    if let Ok(models) = std::env::var("BORG_EVALS_OLLAMA_MODELS") {
        let parsed = models
            .split(',')
            .filter_map(|entry| {
                let model = entry.trim();
                if model.is_empty() {
                    return None;
                }
                Some(ExecutionTarget::ollama(
                    model.replace(':', "-"),
                    model.to_string(),
                ))
            })
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    DEFAULT_OLLAMA_MODELS
        .iter()
        .map(|(label, model)| ExecutionTarget::ollama(*label, *model))
        .collect()
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "borg_evals_core=info,borg_llm_test=info"
                    .parse()
                    .expect("valid env filter")
            }),
        )
        .with_target(false)
        .compact()
        .try_init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let ollama = TestContext::shared(TestProvider::Ollama)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("start shared Ollama test context")?;

    let harness = Arc::new(DateHarness::new(ollama));
    let suite = current_date_tool_suite(harness);
    let targets = default_targets();
    let report = suite
        .run_with(RunConfig::new(targets).with_trials(default_trials()))
        .persist_to(".evals")
        .run()
        .await?;
    let index = report.write_to(".evals")?;

    info!(
        suite = "current-date-tool-baseline",
        variants = report.variants.len(),
        files = index.files.len(),
        "wrote eval artifacts"
    );
    println!("{}", report.summary_table());
    println!("wrote {} artifacts", index.files.len());

    Ok(())
}
