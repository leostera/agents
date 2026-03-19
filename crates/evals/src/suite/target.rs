use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use agents::agent::Agent;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::info;

use super::trial::TrialExecution;
use super::*;
use crate::events::emit;
use crate::report::{IncrementalSuiteWriter, build_summary};

pub(super) async fn run_single_target<State, A>(
    suite: &Suite<State, A>,
    run_id: String,
    target: &ExecutionTarget,
    provider_configs: &ProviderConfigs,
    default_trials: usize,
    timeout: Option<Duration>,
    artifact_root: Option<&Path>,
) -> EvalResult<SuiteRunReport>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    let started_at = now_since_epoch();
    let mut trial_records = Vec::new();
    let total_trial_count = suite
        .evals()
        .iter()
        .map(|eval| eval.configured_trials().unwrap_or(default_trials))
        .sum();

    emit(RunEvent::SuiteStarted {
        suite_id: suite.id().to_string(),
        target_label: target.display_label(),
        eval_count: suite.evals().len(),
        trial_count: total_trial_count,
    });

    info!(
        suite_id = %suite.id(),
        target_label = %target.label,
        provider = %target.provider,
        model = %target.model,
        default_trials,
        max_in_flight = target.max_in_flight,
        "starting suite target"
    );

    let initial_manifest = RunManifest {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        started_at,
        finished_at: started_at,
        suites: vec![suite.id().to_string()],
        targets: vec![target.clone()],
        files: Vec::new(),
    };
    let mut incremental_writer = match artifact_root {
        Some(root) => Some(IncrementalSuiteWriter::new(
            root,
            suite.id(),
            target,
            &initial_manifest,
        )?),
        None => None,
    };

    let semaphore = Arc::new(Semaphore::new(target.max_in_flight.max(1)));
    let mut jobs = JoinSet::new();

    for eval in suite.evals() {
        let trial_count = eval.configured_trials().unwrap_or(default_trials);
        let trial_timeout = eval.configured_timeout().or(timeout);
        info!(
            suite_id = %suite.id(),
            target_label = %target.label,
            eval_id = %eval.id(),
            trials = trial_count,
            "starting eval"
        );
        emit(RunEvent::EvalStarted {
            suite_id: suite.id().to_string(),
            eval_id: eval.id().to_string(),
            target_label: target.display_label(),
            trials: trial_count,
        });
        for trial_index in 0..trial_count {
            let semaphore = semaphore.clone();
            let suite_id = suite.id().to_string();
            let target = target.clone();
            let eval = eval.clone();
            let run_id = run_id.clone();
            let state = suite.shared_state().clone();
            let agent_factory = suite.agent_factory.clone();
            let provider_configs = provider_configs.clone();

            jobs.spawn(async move {
                let _permit = semaphore.acquire_owned().await.expect("semaphore permit");
                TrialExecution {
                    run_id,
                    suite_id,
                    state,
                    target,
                    provider_configs,
                    eval,
                    agent_factory,
                    trial_index,
                    timeout: trial_timeout,
                }
                .run()
                .await
            });
        }
    }

    while let Some(result) = jobs.join_next().await {
        let trial_record = result.expect("trial task panicked");
        if let Some(writer) = incremental_writer.as_mut() {
            writer.write_trial(&trial_record)?;
        }
        trial_records.push(trial_record);
    }

    trial_records.sort_by(|left, right| {
        left.eval_id
            .cmp(&right.eval_id)
            .then(left.trial_index.cmp(&right.trial_index))
    });

    let finished_at = now_since_epoch();
    let summary = build_summary(suite, &run_id, target, &trial_records);
    info!(
        suite_id = %suite.id(),
        target_label = %target.label,
        pass_rate = summary.pass_rate,
        mean_score = summary.mean_score,
        total_trials = summary.total_trials,
        "finished suite target"
    );
    let manifest = RunManifest {
        schema_version: SCHEMA_VERSION,
        run_id,
        started_at,
        finished_at,
        suites: vec![suite.id().to_string()],
        targets: vec![target.clone()],
        files: Vec::new(),
    };

    let report = SuiteRunReport {
        manifest,
        suite: summary,
        trials: trial_records,
    };

    for eval_summary in &report.suite.evals {
        emit(RunEvent::EvalFinished {
            suite_id: report.suite.suite_id.clone(),
            eval_id: eval_summary.eval_id.clone(),
            target_label: report.suite.target.display_label(),
            trial_count: eval_summary.trial_count,
            passed_trials: eval_summary.passed_trials,
            mean_score: eval_summary.mean_score,
            mean_duration_ms: eval_summary.mean_duration.as_millis(),
        });
    }
    emit(RunEvent::SuiteFinished {
        suite_id: report.suite.suite_id.clone(),
        target_label: report.suite.target.display_label(),
        total_trials: report.suite.total_trials,
        passed_trials: report.suite.passed_trials,
        mean_score: report.suite.mean_score,
        mean_duration_ms: report.suite.mean_duration.as_millis(),
    });

    if let Some(writer) = incremental_writer.as_mut() {
        writer.finish(&report)?;
    }

    Ok(report)
}
