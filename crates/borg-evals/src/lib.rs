mod config;
mod error;
mod eval;
mod events;
mod grade;
mod registry;
mod report;
pub mod runner;
mod suite;
mod trajectory;
mod trial;

pub use crate as core;
pub use async_trait::async_trait;
pub use borg_macros::{eval, suite};
pub use config::{ExecutionTarget, RunConfig};
pub use error::{EvalError, EvalResult};
pub use eval::{Eval, EvalContext};
pub use events::{
    EventSink, JsonEventSink, NoopEventSink, PlannedSuiteRun, ProgressEventSink, RunEvent,
    SharedEventSink, emit, global_sink, set_global_sink,
};
pub use grade::{Grade, GradeResult, Grader, GraderFailure, GradingConfig, grade};
pub use registry::{RunnableSuite, SuiteDescriptor, build};
pub use report::{
    ArtifactIndex, EvalAggregate, EvalRunReport, GraderAggregate, RunManifest, SCHEMA_VERSION,
    SuiteRunReport, SuiteSummary, TrialRecord,
};
pub use suite::{Suite, SuiteKind, SuitePlan, TargetFilter};
pub use trajectory::{Step, Trajectory, TrajectoryBuilder};
pub use trial::{
    AgentTrial, AgentTrialRecorder, RecordedEvent, RecordedMessageRole, RecordedToolCall,
};

#[macro_export]
macro_rules! user {
    ($message:expr) => {
        $crate::Step::user($message)
    };
}

#[macro_export]
macro_rules! assistant {
    ($grading:expr) => {
        $grading
    };
}

#[macro_export]
macro_rules! trajectory {
    [
        user!($first_user:expr)
        $(, assistant!($first_grade:expr))?
        $(, user!($next_user:expr) $(, assistant!($next_grade:expr))? )*
        $(,)?
    ] => {{
        $crate::Trajectory::builder()
            .add_step(
                $crate::Step::user($first_user)
                    $(.grade($first_grade))?
            )
            $(
                .add_step(
                    $crate::Step::user($next_user)
                        $(.grade($next_grade))?
                )
            )*
            .build()
            .expect("trajectory! generated an invalid trajectory")
    }};
}

pub mod prelude {
    pub use crate::{
        AgentTrial, AgentTrialRecorder, ArtifactIndex, Eval, EvalAggregate, EvalContext, EvalError,
        EvalResult, EventSink, ExecutionTarget, Grade, GradeResult, Grader, GraderFailure,
        GradingConfig, JsonEventSink, PlannedSuiteRun, ProgressEventSink, RecordedEvent,
        RecordedMessageRole, RecordedToolCall, RunConfig, RunEvent, RunnableSuite, SharedEventSink,
        Step, Suite, SuiteDescriptor, SuiteKind, SuitePlan, SuiteRunReport, TargetFilter,
        Trajectory, TrajectoryBuilder, assistant, async_trait, build, emit, global_sink, grade,
        set_global_sink, setup, trajectory, user,
    };
    pub use borg_agent::Agent;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use borg_agent::{AgentError, AgentEvent, AgentInput, AgentRunInput, AgentRunOutput};
    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, Instant};

    use crate::prelude::*;

    #[derive(Clone, Debug)]
    struct DummyAgent;

    #[async_trait]
    impl Agent for DummyAgent {
        type Input = serde_json::Value;
        type ToolCall = serde_json::Value;
        type ToolResult = serde_json::Value;
        type Output = String;

        async fn send(&mut self, _input: AgentInput<Self::Input>) -> Result<(), AgentError> {
            Err(AgentError::Internal {
                message: "dummy agent should not be run directly".to_string(),
            })
        }

        async fn next(
            &mut self,
        ) -> Result<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>, AgentError>
        {
            Err(AgentError::Internal {
                message: "dummy agent should not be run directly".to_string(),
            })
        }

        async fn spawn(
            self,
        ) -> Result<
            (
                borg_agent::AgentRunInput<Self::Input>,
                borg_agent::AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
            ),
            AgentError,
        > {
            Err(AgentError::Internal {
                message: "dummy agent should not be run directly".to_string(),
            })
        }
    }

    fn suite_with_dummy_agent(id: &str) -> Suite<(), DummyAgent> {
        Suite::new(id).agent(|_ctx| async move { Ok::<DummyAgent, EvalError>(DummyAgent) })
    }

    #[derive(Clone)]
    struct EchoAgent;

    #[async_trait]
    impl Agent for EchoAgent {
        type Input = String;
        type ToolCall = ();
        type ToolResult = ();
        type Output = String;

        async fn send(&mut self, _input: AgentInput<Self::Input>) -> Result<(), AgentError> {
            Err(AgentError::Internal {
                message: "echo test agent only supports spawn".to_string(),
            })
        }

        async fn next(
            &mut self,
        ) -> Result<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>, AgentError>
        {
            Err(AgentError::Internal {
                message: "echo test agent only supports spawn".to_string(),
            })
        }

        async fn spawn(
            self,
        ) -> Result<
            (
                AgentRunInput<Self::Input>,
                AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
            ),
            AgentError,
        > {
            let (input_tx, mut input_rx) = mpsc::channel(16);
            let (event_tx, event_rx) = mpsc::channel(16);

            tokio::spawn(async move {
                while let Some(input) = input_rx.recv().await {
                    match input {
                        AgentInput::Message(text) | AgentInput::Steer(text) => {
                            let _ = event_tx
                                .send(Ok(AgentEvent::Completed { reply: text }))
                                .await;
                        }
                        AgentInput::Cancel => {
                            let _ = event_tx.send(Ok(AgentEvent::Cancelled)).await;
                            break;
                        }
                    }
                }
            });

            Ok((input_tx, event_rx))
        }
    }

    #[tokio::test]
    async fn suite_runs_trials_and_aggregates_scores() {
        let suite = suite_with_dummy_agent("calendar").trials(2).eval(
            Eval::new("happy-path")
                .grading(grade("reply-is-done", |trial, _ctx| async move {
                    Ok(GradeResult {
                        score: if trial.final_reply.as_deref() == Some("done") {
                            1.0
                        } else {
                            0.0
                        },
                        summary: "reply should equal done".to_string(),
                        evidence: json!({ "reply": trial.final_reply }),
                    })
                }))
                .run(|ctx, _agent| async move {
                    Ok(AgentTrial {
                        transcript: vec![RecordedEvent::Message {
                            role: RecordedMessageRole::User,
                            content: format!("trial {}", ctx.trial_index),
                        }],
                        final_reply: Some("done".to_string()),
                        tool_trace: Vec::new(),
                        grades: BTreeMap::new(),
                        grader_failures: Vec::new(),
                        metadata: json!({ "trial_index": ctx.trial_index }),
                    })
                }),
        );

        let report = suite.run().await.expect("suite to run");
        assert_eq!(report.suite.total_trials, 2);
        assert_eq!(report.suite.passed_trials, 2);
        assert_eq!(report.suite.evals.len(), 1);
        assert_eq!(report.trials.len(), 2);
        assert!(report.trials.iter().all(|trial| trial.trial.is_some()));
    }

    #[tokio::test]
    async fn suite_runner_expands_model_matrix() {
        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("matrix")
                .grading(grade("always", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run(|ctx, _agent| async move {
                    Ok(AgentTrial::new(format!("target={}", ctx.target.label)))
                }),
        );

        let report = suite
            .run_with(
                RunConfig::new(vec![
                    ExecutionTarget::ollama("qwen2.5", "qwen2.5:7b"),
                    ExecutionTarget::ollama("qwen3.5", "qwen3.5"),
                ])
                .with_trials(2),
            )
            .run()
            .await
            .expect("matrix run");

        assert_eq!(report.variants.len(), 2);
        assert_eq!(report.variants[0].suite.target.label, "qwen2.5");
        assert_eq!(report.variants[1].suite.target.label, "qwen3.5");
        assert_eq!(report.manifest.targets.len(), 2);
    }

    #[test]
    fn suite_runner_plan_filters_evals_and_targets_before_execution() {
        let suite = suite_with_dummy_agent("echo")
            .eval(Eval::new("echoes_plain_text"))
            .eval(Eval::new("preserves_newlines"))
            .eval(Eval::new("preserves_empty_string"));

        let plan = suite
            .run_with(RunConfig::new(vec![
                ExecutionTarget::ollama("llama3.2:1b", "llama3.2:1b"),
                ExecutionTarget::ollama("llama3.2:3b", "llama3.2:3b"),
            ]))
            .filter(TargetFilter {
                query: Some("preserves".to_string()),
                model: Some("ollama/llama3.2:1b".to_string()),
            })
            .plan()
            .expect("plan to succeed");

        let eval_ids = plan
            .suite()
            .evals()
            .iter()
            .map(|eval| eval.id().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            eval_ids,
            vec!["preserves_newlines", "preserves_empty_string"]
        );
        assert_eq!(plan.config().targets.len(), 1);
        assert_eq!(plan.config().targets[0].label, "llama3.2:1b");
    }

    #[test]
    fn suite_runner_plan_errors_when_no_query_matches() {
        let suite = suite_with_dummy_agent("echo").eval(Eval::new("echoes_plain_text"));

        let error = suite
            .run_with(RunConfig::single(ExecutionTarget::ollama(
                "llama3.2:1b",
                "llama3.2:1b",
            )))
            .filter(TargetFilter {
                query: Some("preserves".to_string()),
                model: None,
            })
            .plan()
            .expect_err("plan to fail");

        assert_eq!(
            error.to_string(),
            "eval failed: no suites, models, or evals matched query \"preserves\""
        );
    }

    #[test]
    fn suite_runner_plan_errors_when_no_model_matches() {
        let suite = suite_with_dummy_agent("echo").eval(Eval::new("echoes_plain_text"));

        let error = suite
            .run_with(RunConfig::single(ExecutionTarget::ollama(
                "llama3.2:1b",
                "llama3.2:1b",
            )))
            .filter(TargetFilter {
                query: None,
                model: Some("ollama/llama3.2:3b".to_string()),
            })
            .plan()
            .expect_err("plan to fail");

        assert_eq!(
            error.to_string(),
            "eval failed: no eval targets matched model \"ollama/llama3.2:3b\""
        );
    }

    #[test]
    fn suite_runner_plan_keeps_full_suite_when_query_matches_suite_id() {
        let suite = suite_with_dummy_agent("echo")
            .eval(Eval::new("echoes_plain_text"))
            .eval(Eval::new("preserves_newlines"));

        let plan = suite
            .run_with(RunConfig::single(ExecutionTarget::ollama(
                "llama3.2:1b",
                "llama3.2:1b",
            )))
            .filter(TargetFilter {
                query: Some("echo".to_string()),
                model: None,
            })
            .plan()
            .expect("plan to succeed");

        let eval_ids = plan
            .suite()
            .evals()
            .iter()
            .map(|eval| eval.id().to_string())
            .collect::<Vec<_>>();

        assert_eq!(eval_ids, vec!["echoes_plain_text", "preserves_newlines"]);
    }

    #[test]
    fn eval_tags_builder_extends_tags_in_order() {
        let eval: Eval<()> = Eval::new("calendar")
            .tags(["calendar", "free-time"])
            .tag("regression");

        assert_eq!(
            eval.tag_list(),
            &[
                "calendar".to_string(),
                "free-time".to_string(),
                "regression".to_string(),
            ]
        );
    }

    #[test]
    fn summary_markdown_prefixes_non_local_providers() {
        let hosted_target = ExecutionTarget::openrouter("kimi-k2.5", "moonshotai/kimi-k2.5");
        let local_target = ExecutionTarget::ollama("qwen3.5", "qwen3.5");

        assert_eq!(hosted_target.display_label(), "openrouter:kimi-k2.5");
        assert_eq!(local_target.display_label(), "qwen3.5");
    }

    #[tokio::test]
    async fn summary_table_groups_rows_by_eval() {
        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("compress-day")
                .grading(grade("free-block", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run(|_, _agent| async move { Ok(AgentTrial::new("ok".to_string())) }),
        );

        let report = suite
            .run_with(
                RunConfig::single(ExecutionTarget::openrouter(
                    "kimi-k2.5",
                    "moonshotai/kimi-k2.5",
                ))
                .with_trials(2),
            )
            .run()
            .await
            .expect("table report");

        let table = report.summary_table();
        assert!(table.contains("== Eval: compress-day (~2 trials) =="));
        assert!(table.contains("avg duration ⏱"));
        assert!(table.contains("final 🏁"));
        assert!(table.contains("grades 🔎"));
        assert!(table.contains("openrouter:kimi-k2.5"));
        assert!(table.contains("ms  🥇") || table.contains("ms  🥈") || table.contains("ms  🥉"));
        assert!(table.contains("free-block"));
        assert!(table.contains("1.00"));
        assert!(table.contains("🥇"));
    }

    #[tokio::test]
    async fn grader_means_count_failed_trials_as_zero_score() {
        let suite = suite_with_dummy_agent("calendar").trials(2).eval(
            Eval::new("compress-day")
                .grading(grade("free-block", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run(|ctx, _agent| async move {
                    if ctx.trial_index == 0 {
                        Err(EvalError::message("trial failed"))
                    } else {
                        Ok(AgentTrial::new("ok".to_string()))
                    }
                }),
        );

        let report = suite.run().await.expect("report");
        let eval = &report.suite.evals[0];
        assert_eq!(eval.mean_score, 0.5);
        assert_eq!(eval.grader_means[0].mean_score, 0.5);
        assert_eq!(eval.grader_means[0].pass_rate, 0.5);
    }

    #[tokio::test]
    async fn suite_records_failed_trials_without_aborting_run() {
        let suite = suite_with_dummy_agent("calendar")
            .trials(2)
            .eval(Eval::new("fails").run(|ctx, _agent| async move {
                if ctx.trial_index == 0 {
                    Err(EvalError::message("llm exploded"))
                } else {
                    Ok(AgentTrial::new("recovered".to_string()))
                }
            }))
            .eval(
                Eval::new("still-runs")
                    .grading(grade("always", |_, _ctx| async move {
                        Ok(GradeResult {
                            score: 1.0,
                            summary: "always".to_string(),
                            evidence: serde_json::Value::Null,
                        })
                    }))
                    .run(|_, _agent| async move { Ok(AgentTrial::new("ok".to_string())) }),
            );

        let report = suite.run().await.expect("suite should not abort");
        assert_eq!(report.trials.len(), 4);
        assert!(report.trials.iter().any(|trial| trial.error.is_some()));
        assert!(
            report
                .trials
                .iter()
                .any(|trial| trial.eval_id == "still-runs" && trial.passed)
        );
    }

    #[tokio::test]
    async fn trajectory_expectations_appear_in_trial_and_summary_grades() {
        let suite = Suite::new("echo")
            .agent(|_ctx| async move { Ok::<EchoAgent, EvalError>(EchoAgent) })
            .eval(
                Eval::new("echoes").run(
                    Trajectory::<EchoAgent>::builder()
                        .add_step(Step::user("hello".to_string()).grade(
                            GradingConfig::new().grade("echoes-hello", |trial, _ctx| async move {
                                Ok(GradeResult {
                                    score: if trial.final_reply.as_deref() == Some("hello") {
                                        1.0
                                    } else {
                                        0.0
                                    },
                                    summary: "reply should equal hello".to_string(),
                                    evidence: json!({ "reply": trial.final_reply }),
                                })
                            }),
                        ))
                        .build()
                        .expect("trajectory")
                        .runner(),
                ),
            );

        let report = suite
            .run_with(RunConfig::single(ExecutionTarget::ollama("echo", "echo")).with_trials(1))
            .run()
            .await
            .expect("report");
        let variant = &report.variants[0];

        assert_eq!(variant.trials.len(), 1);
        assert_eq!(variant.trials[0].grades.len(), 1);
        assert!(variant.trials[0].grades.contains_key("echoes-hello"));
        assert_eq!(variant.suite.evals[0].grader_means.len(), 1);
        assert_eq!(variant.suite.evals[0].grader_means[0].name, "echoes-hello");
    }

    #[test]
    fn trajectory_macro_builds_linear_steps() {
        let grading = GradingConfig::<(), String>::new().grade("always", |_, _| async move {
            Ok(GradeResult {
                score: 1.0,
                summary: "always".to_string(),
                evidence: serde_json::Value::Null,
            })
        });

        let trajectory: Trajectory<EchoAgent> = trajectory![
            user!("hello".to_string()),
            assistant!(grading.clone()),
            user!("world".to_string()),
            assistant!(grading),
        ];

        assert_eq!(trajectory.steps().len(), 2);
    }

    #[tokio::test]
    async fn failed_trials_can_persist_partial_agent_output() {
        let suite = suite_with_dummy_agent("calendar").eval(Eval::new("partial").run(
            |_, _agent| async move {
                Err(EvalError::message_with_trial(
                    "agent never finished",
                    AgentTrial::<String> {
                        transcript: vec![RecordedEvent::Message {
                            role: RecordedMessageRole::Assistant,
                            content: "working on it".to_string(),
                        }],
                        final_reply: None,
                        tool_trace: Vec::new(),
                        grades: BTreeMap::new(),
                        grader_failures: Vec::new(),
                        metadata: json!({ "partial": true }),
                    },
                ))
            },
        ));

        let report = suite.run().await.expect("suite should not abort");
        let trial = &report.trials[0];
        assert_eq!(
            trial.error.as_deref(),
            Some("eval failed: agent never finished")
        );
        assert!(trial.trial.is_some());
        assert_eq!(
            trial
                .trial
                .as_ref()
                .and_then(|trial| trial.get("final_reply")),
            Some(&serde_json::Value::Null)
        );
    }

    #[tokio::test]
    async fn trial_records_capture_timing_and_summary_averages() {
        let suite = suite_with_dummy_agent("calendar")
            .trials(2)
            .eval(Eval::new("timed").run(|_, _agent| async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(AgentTrial::new("ok".to_string()))
            }));

        let report = suite.run().await.expect("timed suite to run");
        assert_eq!(report.trials.len(), 2);
        assert!(
            report
                .trials
                .iter()
                .all(|trial| trial.finished_at >= trial.started_at)
        );
        assert!(
            report
                .trials
                .iter()
                .all(|trial| trial.duration > Duration::ZERO)
        );
        assert!(report.suite.mean_duration > Duration::ZERO);
        assert!(report.suite.evals[0].mean_duration > Duration::ZERO);
        assert!(report.summary_markdown().contains("mean duration:"));
    }

    #[tokio::test]
    async fn report_writer_persists_expected_files() {
        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("write-artifacts")
                .grading(grade("passes", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run(|_, _agent| async move { Ok(AgentTrial::new("ok".to_string())) }),
        );

        let report = suite.run().await.expect("suite to run");
        let root = unique_test_dir("borg-evals");
        let index = report.write_to(&root).expect("artifacts to write");

        assert!(root.join(&index.files[0]).exists());
        assert!(root.join("results/calendar").exists());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[tokio::test]
    async fn hosted_targets_run_trials_concurrently_within_limit() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("parallel")
                .grading(grade("always", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run({
                    let in_flight = in_flight.clone();
                    let peak = peak.clone();
                    move |_, _agent| {
                        let in_flight = in_flight.clone();
                        let peak = peak.clone();
                        async move {
                            let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(current, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            in_flight.fetch_sub(1, Ordering::SeqCst);
                            Ok(AgentTrial::new("ok".to_string()))
                        }
                    }
                }),
        );

        let started = Instant::now();
        let report = suite
            .run_with(
                RunConfig::single(
                    ExecutionTarget::openai("gpt", "gpt-5.3-codex").with_max_in_flight(4),
                )
                .with_trials(8),
            )
            .run()
            .await
            .expect("concurrent run");
        let elapsed = started.elapsed();

        assert_eq!(report.variants.len(), 1);
        assert_eq!(report.variants[0].trials.len(), 8);
        assert!(peak.load(Ordering::SeqCst) > 1);
        assert!(elapsed < Duration::from_millis(300));
    }

    #[tokio::test]
    async fn hosted_targets_overlap_with_local_targets_while_local_targets_serialize() {
        let local_in_flight = Arc::new(AtomicUsize::new(0));
        let local_peak = Arc::new(AtomicUsize::new(0));
        let hosted_saw_local_running = Arc::new(AtomicBool::new(false));

        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("mixed-targets")
                .grading(grade("always", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run({
                    let local_in_flight = local_in_flight.clone();
                    let local_peak = local_peak.clone();
                    let hosted_saw_local_running = hosted_saw_local_running.clone();
                    move |ctx, _agent| {
                        let local_in_flight = local_in_flight.clone();
                        let local_peak = local_peak.clone();
                        let hosted_saw_local_running = hosted_saw_local_running.clone();
                        async move {
                            if ctx.target.provider == "ollama" {
                                let current = local_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                                local_peak.fetch_max(current, Ordering::SeqCst);
                                tokio::time::sleep(Duration::from_millis(120)).await;
                                local_in_flight.fetch_sub(1, Ordering::SeqCst);
                            } else {
                                if local_in_flight.load(Ordering::SeqCst) > 0 {
                                    hosted_saw_local_running.store(true, Ordering::SeqCst);
                                }
                                tokio::time::sleep(Duration::from_millis(20)).await;
                                if local_in_flight.load(Ordering::SeqCst) > 0 {
                                    hosted_saw_local_running.store(true, Ordering::SeqCst);
                                }
                            }
                            Ok(AgentTrial::new("ok".to_string()))
                        }
                    }
                }),
        );

        let report = suite
            .run_with(
                RunConfig::new(vec![
                    ExecutionTarget::ollama("llama3.1", "llama3.1:8b"),
                    ExecutionTarget::openai("gpt", "gpt-5.3-codex"),
                    ExecutionTarget::ollama("qwen3.5", "qwen3.5"),
                ])
                .with_trials(1),
            )
            .run()
            .await
            .expect("mixed run");

        assert_eq!(report.variants.len(), 3);
        assert_eq!(local_peak.load(Ordering::SeqCst), 1);
        assert!(hosted_saw_local_running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn persisted_runs_flush_trial_records_before_completion() {
        let root = unique_test_dir("borg-evals-incremental");
        let suite = suite_with_dummy_agent("calendar").eval(
            Eval::new("incremental")
                .grading(grade("always", |_, _ctx| async move {
                    Ok(GradeResult {
                        score: 1.0,
                        summary: "always".to_string(),
                        evidence: serde_json::Value::Null,
                    })
                }))
                .run(|ctx, _agent| async move {
                    if ctx.trial_index == 0 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    Ok(AgentTrial::new("ok".to_string()))
                }),
        );

        let run = suite
            .run_with(
                RunConfig::single(
                    ExecutionTarget::openai("gpt", "gpt-5.3-codex").with_max_in_flight(1),
                )
                .with_trials(2),
            )
            .persist_to(&root)
            .run();

        let check_persisted_trial = async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            let results_dir = root.join("results").join("calendar");
            let run_dir = fs::read_dir(&results_dir)
                .expect("results dir should exist")
                .next()
                .expect("run dir entry")
                .expect("run dir");
            let trial_path = run_dir
                .path()
                .join("gpt")
                .read_dir()
                .expect("target dir should exist")
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("trial-001__incremental__"))
                })
                .expect("expected first trial artifact to exist before full run completed");
            assert!(trial_path.exists());
        };

        let (report, ()) = tokio::join!(run, check_persisted_trial);
        let report = report.expect("persisted run");
        assert_eq!(report.variants[0].trials.len(), 2);

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            crate::report::now_since_epoch().as_millis()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
