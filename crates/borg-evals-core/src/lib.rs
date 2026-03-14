mod case;
mod config;
mod error;
mod grade;
mod report;
mod suite;
mod trial;

pub use case::{Case, TrialContext};
pub use config::{ExecutionTarget, RunConfig};
pub use error::{EvalError, EvalResult};
pub use grade::{GradeResult, Grader, grade};
pub use report::{
    ArtifactIndex, CaseAggregate, EvalRunReport, GraderAggregate, RunManifest, SCHEMA_VERSION,
    SuiteRunReport, SuiteSummary, TrialRecord,
};
pub use suite::{Suite, SuiteKind};
pub use trial::{AgentTrial, RecordedEvent, RecordedMessageRole, RecordedToolCall};

pub mod prelude {
    pub use crate::{
        AgentTrial, ArtifactIndex, Case, EvalError, EvalResult, ExecutionTarget, GradeResult,
        Grader, RecordedEvent, RecordedMessageRole, RecordedToolCall, RunConfig, Suite, SuiteKind,
        SuiteRunReport, TrialContext, grade,
    };
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use serde_json::json;
    use tokio::time::{Duration, Instant};

    use crate::prelude::*;

    #[tokio::test]
    async fn suite_runs_trials_and_aggregates_scores() {
        let suite = Suite::new("calendar").trials(2).case(
            Case::new("happy-path")
                .run(|ctx| async move {
                    Ok(AgentTrial {
                        transcript: vec![RecordedEvent::Message {
                            role: RecordedMessageRole::User,
                            content: format!("trial {}", ctx.trial_index),
                        }],
                        final_reply: "done".to_string(),
                        tool_trace: Vec::new(),
                        metadata: json!({ "trial_index": ctx.trial_index }),
                    })
                })
                .grade(grade("reply-is-done", |trial| async move {
                    Ok(GradeResult::pass_if(
                        "reply-is-done",
                        trial.final_reply == "done",
                        "reply should equal done",
                        json!({ "reply": trial.final_reply }),
                    ))
                })),
        );

        let report = suite.run().await.expect("suite to run");
        assert_eq!(report.suite.total_trials, 2);
        assert_eq!(report.suite.passed_trials, 2);
        assert_eq!(report.suite.cases.len(), 1);
        assert_eq!(report.trials.len(), 2);
        assert!(report.trials.iter().all(|trial| trial.trial.is_some()));
    }

    #[tokio::test]
    async fn suite_runner_expands_model_matrix() {
        let suite =
            Suite::new("calendar").case(
                Case::new("matrix")
                    .run(|ctx| async move {
                        Ok(AgentTrial::new(format!("target={}", ctx.target.label)))
                    })
                    .grade(grade("always", |_| async move {
                        Ok(GradeResult::pass("always", "always"))
                    })),
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
    fn summary_markdown_prefixes_non_local_providers() {
        let hosted_target = ExecutionTarget::openrouter("kimi-k2.5", "moonshotai/kimi-k2.5");
        let local_target = ExecutionTarget::ollama("qwen3.5", "qwen3.5");

        assert_eq!(hosted_target.display_label(), "openrouter:kimi-k2.5");
        assert_eq!(local_target.display_label(), "qwen3.5");
    }

    #[tokio::test]
    async fn summary_table_groups_rows_by_case() {
        let suite = Suite::new("calendar").case(
            Case::new("compress-day")
                .run(|_| async move { Ok(AgentTrial::new("ok")) })
                .grade(grade("free-block", |_| async move {
                    Ok(GradeResult::pass("free-block", "always"))
                })),
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
        assert!(table.contains("== Case: compress-day (~2 trials)"));
        assert!(table.contains("avg duration ⏱"));
        assert!(table.contains("final 🏁"));
        assert!(table.contains("grades 🔎"));
        assert!(table.contains("openrouter:kimi-k2.5"));
        assert!(table.contains("free-block"));
        assert!(table.contains("1.00"));
        assert!(table.contains("🥇"));
    }

    #[tokio::test]
    async fn grader_means_count_failed_trials_as_zero_score() {
        let suite = Suite::new("calendar").trials(2).case(
            Case::new("compress-day")
                .run(|ctx| async move {
                    if ctx.trial_index == 0 {
                        Err(EvalError::message("trial failed"))
                    } else {
                        Ok(AgentTrial::new("ok"))
                    }
                })
                .grade(grade("free-block", |_| async move {
                    Ok(GradeResult::pass("free-block", "always"))
                })),
        );

        let report = suite.run().await.expect("report");
        let case = &report.suite.cases[0];
        assert_eq!(case.mean_score, 0.5);
        assert_eq!(case.grader_means[0].mean_score, 0.5);
        assert_eq!(case.grader_means[0].pass_rate, 0.5);
    }

    #[tokio::test]
    async fn suite_records_failed_trials_without_aborting_run() {
        let suite = Suite::new("calendar")
            .trials(2)
            .case(Case::new("fails").run(|ctx| async move {
                if ctx.trial_index == 0 {
                    Err(EvalError::message("llm exploded"))
                } else {
                    Ok(AgentTrial::new("recovered"))
                }
            }))
            .case(
                Case::new("still-runs")
                    .run(|_| async move { Ok(AgentTrial::new("ok")) })
                    .grade(grade("always", |_| async move {
                        Ok(GradeResult::pass("always", "always"))
                    })),
            );

        let report = suite.run().await.expect("suite should not abort");
        assert_eq!(report.trials.len(), 4);
        assert!(report.trials.iter().any(|trial| trial.error.is_some()));
        assert!(
            report
                .trials
                .iter()
                .any(|trial| trial.case_id == "still-runs" && trial.passed)
        );
    }

    #[tokio::test]
    async fn trial_records_capture_timing_and_summary_averages() {
        let suite = Suite::new("calendar").trials(2).case(
            Case::new("timed").run(|_| async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(AgentTrial::new("ok"))
            }),
        );

        let report = suite.run().await.expect("timed suite to run");
        assert_eq!(report.trials.len(), 2);
        assert!(report.trials.iter().all(|trial| trial.finished_at >= trial.started_at));
        assert!(report.trials.iter().all(|trial| trial.duration > Duration::ZERO));
        assert!(report.suite.mean_duration > Duration::ZERO);
        assert!(report.suite.cases[0].mean_duration > Duration::ZERO);
        assert!(report.summary_markdown().contains("mean duration:"));
    }

    #[tokio::test]
    async fn report_writer_persists_expected_files() {
        let suite = Suite::new("calendar").case(
            Case::new("write-artifacts")
                .run(|_| async move { Ok(AgentTrial::new("ok")) })
                .grade(grade("passes", |_| async move {
                    Ok(GradeResult::pass("passes", "always"))
                })),
        );

        let report = suite.run().await.expect("suite to run");
        let root = unique_test_dir("borg-evals-core");
        let index = report.write_to(&root).expect("artifacts to write");

        assert!(root.join(&index.files[0]).exists());
        assert!(root.join("results/calendar").exists());

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[tokio::test]
    async fn hosted_targets_run_trials_concurrently_within_limit() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let suite = Suite::new("calendar").case(
            Case::new("parallel")
                .run({
                    let in_flight = in_flight.clone();
                    let peak = peak.clone();
                    move |_| {
                        let in_flight = in_flight.clone();
                        let peak = peak.clone();
                        async move {
                            let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(current, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            in_flight.fetch_sub(1, Ordering::SeqCst);
                            Ok(AgentTrial::new("ok"))
                        }
                    }
                })
                .grade(grade("always", |_| async move {
                    Ok(GradeResult::pass("always", "always"))
                })),
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

        let suite = Suite::new("calendar").case(
            Case::new("mixed-targets")
                .run({
                    let local_in_flight = local_in_flight.clone();
                    let local_peak = local_peak.clone();
                    let hosted_saw_local_running = hosted_saw_local_running.clone();
                    move |ctx| {
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
                            Ok(AgentTrial::new("ok"))
                        }
                    }
                })
                .grade(grade("always", |_| async move {
                    Ok(GradeResult::pass("always", "always"))
                })),
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
        let root = unique_test_dir("borg-evals-core-incremental");
        let suite = Suite::new("calendar").case(
            Case::new("incremental")
                .run(|ctx| async move {
                    if ctx.trial_index == 0 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    Ok(AgentTrial::new("ok"))
                })
                .grade(grade("always", |_| async move {
                    Ok(GradeResult::pass("always", "always"))
                })),
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
                .join("trial-001__incremental.json");
            assert!(
                trial_path.exists(),
                "expected first trial artifact to exist before full run completed: {}",
                trial_path.display()
            );
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
