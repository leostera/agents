use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agents::agent::Agent;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::*;
use crate::events::emit;
use crate::grade::is_passing_score;
use crate::report::{TrialRecord, usage_summary_from_transcript};
use crate::trial::RecordedError;

pub(super) struct TrialExecution<State, A>
where
    A: Agent,
{
    pub(super) run_id: String,
    pub(super) suite_id: String,
    pub(super) state: Arc<State>,
    pub(super) target: ExecutionTarget,
    pub(super) provider_configs: ProviderConfigs,
    pub(super) eval: Eval<State, A>,
    pub(super) agent_factory: Option<AgentFactory<State, A>>,
    pub(super) trial_index: usize,
    pub(super) timeout: Option<Duration>,
}

impl<State, A> TrialExecution<State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    pub(super) async fn run(self) -> TrialRecord {
        let trial_id = Uuid::now_v7().to_string();
        let started_at_wall = now_since_epoch();
        let started_at_instant = Instant::now();
        let llm_runner = match llm_runner_for_target(&self.target, &self.provider_configs) {
            Ok(runner) => Arc::new(runner),
            Err(error) => {
                let finished_at_wall = now_since_epoch();
                let record = TrialRecord {
                    schema_version: SCHEMA_VERSION,
                    trial_id: trial_id.clone(),
                    run_id: self.run_id,
                    suite_id: self.suite_id,
                    target: self.target,
                    eval_id: self.eval.id().to_string(),
                    trial_index: self.trial_index,
                    started_at: started_at_wall,
                    finished_at: finished_at_wall,
                    duration: started_at_instant.elapsed(),
                    passed: false,
                    mean_score: 0.0,
                    usage: crate::report::UsageSummary::default(),
                    trial: None,
                    error: Some(RecordedError::from_eval_error(&error)),
                    grades: BTreeMap::new(),
                    grader_failures: Vec::new(),
                };
                emit(RunEvent::TrialFinished {
                    suite_id: record.suite_id.clone(),
                    eval_id: record.eval_id.clone(),
                    trial_id: record.trial_id.clone(),
                    trial_index: record.trial_index,
                    target_label: record.target.display_label(),
                    passed: record.passed,
                    mean_score: record.mean_score,
                    duration_ms: record.duration.as_millis(),
                    error: record.error.as_ref().map(ToString::to_string),
                });
                return record;
            }
        };
        let ctx = EvalContext {
            suite_id: self.suite_id.clone(),
            eval_id: self.eval.id().to_string(),
            trial_id: trial_id.clone(),
            trial_index: self.trial_index,
            target: self.target.clone(),
            llm_runner,
            state: self.state,
        };
        debug!(
            suite_id = %self.suite_id,
            target_label = %self.target.label,
            eval_id = %self.eval.id(),
            trial_id = %trial_id,
            trial_index = self.trial_index,
            "starting trial"
        );

        let execution_future = async {
            match self.agent_factory {
                Some(factory) => match factory(ctx.clone()).await {
                    Ok(agent) => {
                        debug!(
                            suite_id = %self.suite_id,
                            target_label = %self.target.label,
                            eval_id = %self.eval.id(),
                            trial_id = %trial_id,
                            trial_index = self.trial_index,
                            "agent built"
                        );
                        self.eval.execute(ctx.clone(), agent).await
                    }
                    Err(error) => Err(error),
                },
                None => Err(EvalError::message("suite missing agent factory")),
            }
        };

        let execution = match self.timeout {
            Some(timeout) => match tokio::time::timeout(timeout, execution_future).await {
                Ok(result) => result,
                Err(_) => Err(EvalError::trial_timed_out(timeout)),
            },
            None => execution_future.await,
        };

        match execution {
            Ok(trial) => {
                let mut trial = trial;
                let trajectory_grades = trial.grades.clone();
                let trajectory_grader_failures = trial.grader_failures.clone();
                let outcome = self
                    .eval
                    .grading_config()
                    .run_with_scope(
                        trial.clone(),
                        ctx.clone(),
                        crate::trial::RecordedGradingScope::Eval,
                    )
                    .await;
                let (passed, mean_score, grades, grader_failures) = match outcome {
                    Ok(outcome) => {
                        trial.transcript.extend(outcome.recorded_events);
                        let mut grades = trajectory_grades.clone();
                        grades.extend(outcome.grades);
                        let mut grader_failures = trajectory_grader_failures.clone();
                        grader_failures.extend(outcome.grader_failures);
                        let configured_grader_count = grades.len() + grader_failures.len();
                        let passed = grader_failures.is_empty()
                            && grades.values().all(|grade| is_passing_score(grade.score));
                        let mean_score = if configured_grader_count == 0 {
                            1.0
                        } else {
                            grades.values().map(|grade| grade.score).sum::<f32>()
                                / configured_grader_count as f32
                        };
                        (passed, mean_score, grades, grader_failures)
                    }
                    Err(error) => (false, 0.0, trajectory_grades.clone(), {
                        let mut grader_failures = trajectory_grader_failures.clone();
                        grader_failures.push(crate::grade::GraderFailure {
                            name: "grading".to_string(),
                            error: RecordedError::from_eval_error(&error),
                        });
                        grader_failures
                    }),
                };

                let error = if grader_failures.is_empty() {
                    None
                } else {
                    Some(RecordedError::eval_message(format!(
                        "{} graders failed",
                        grader_failures.len()
                    )))
                };

                if let Some(error) = &error {
                    warn!(
                        suite_id = %self.suite_id,
                        target_label = %self.target.label,
                        eval_id = %self.eval.id(),
                        trial_id = %trial_id,
                        trial_index = self.trial_index,
                        %error,
                        "trial grading failed"
                    );
                } else {
                    info!(
                        suite_id = %self.suite_id,
                        target_label = %self.target.label,
                        eval_id = %self.eval.id(),
                        trial_id = %trial_id,
                        trial_index = self.trial_index,
                        passed,
                        mean_score,
                        "finished trial"
                    );
                }

                let finished_at_wall = now_since_epoch();
                let usage = usage_summary_from_transcript(&trial.transcript);

                let record = TrialRecord {
                    schema_version: SCHEMA_VERSION,
                    trial_id,
                    run_id: self.run_id,
                    suite_id: self.suite_id,
                    target: self.target,
                    eval_id: self.eval.id().to_string(),
                    trial_index: self.trial_index,
                    started_at: started_at_wall,
                    finished_at: finished_at_wall,
                    duration: started_at_instant.elapsed(),
                    passed,
                    mean_score,
                    usage,
                    trial: Some(serde_json::to_value(&trial).expect("serialize trial")),
                    error,
                    grades,
                    grader_failures,
                };
                emit(RunEvent::TrialFinished {
                    suite_id: record.suite_id.clone(),
                    eval_id: record.eval_id.clone(),
                    trial_id: record.trial_id.clone(),
                    trial_index: record.trial_index,
                    target_label: record.target.display_label(),
                    passed: record.passed,
                    mean_score: record.mean_score,
                    duration_ms: record.duration.as_millis(),
                    error: record.error.as_ref().map(ToString::to_string),
                });
                record
            }
            Err(error) => {
                error!(
                    suite_id = %self.suite_id,
                    target_label = %self.target.label,
                    eval_id = %self.eval.id(),
                    trial_id = %trial_id,
                    trial_index = self.trial_index,
                    error = %error,
                    "trial execution failed"
                );
                let finished_at_wall = now_since_epoch();
                let partial_trial = error.partial_trial_json().cloned();
                let (grades, grader_failures, mean_score) = partial_trial
                    .as_ref()
                    .and_then(|trial| {
                        serde_json::from_value::<crate::trial::AgentTrial<serde_json::Value>>(
                            trial.clone(),
                        )
                        .ok()
                    })
                    .map(|trial| {
                        let grader_count = trial.grades.len() + trial.grader_failures.len();
                        let mean_score = if grader_count == 0 {
                            0.0
                        } else {
                            trial.grades.values().map(|grade| grade.score).sum::<f32>()
                                / grader_count as f32
                        };
                        (trial.grades, trial.grader_failures, mean_score)
                    })
                    .unwrap_or_else(|| (BTreeMap::new(), Vec::new(), 0.0));
                let record = TrialRecord {
                    schema_version: SCHEMA_VERSION,
                    trial_id,
                    run_id: self.run_id,
                    suite_id: self.suite_id,
                    target: self.target,
                    eval_id: self.eval.id().to_string(),
                    trial_index: self.trial_index,
                    started_at: started_at_wall,
                    finished_at: finished_at_wall,
                    duration: started_at_instant.elapsed(),
                    passed: false,
                    mean_score,
                    usage: partial_trial
                        .as_ref()
                        .and_then(|trial| {
                            serde_json::from_value::<crate::trial::AgentTrial<serde_json::Value>>(
                                trial.clone(),
                            )
                            .ok()
                        })
                        .map(|trial| usage_summary_from_transcript(&trial.transcript))
                        .unwrap_or_default(),
                    trial: partial_trial,
                    error: Some(RecordedError::from_eval_error(&error)),
                    grades,
                    grader_failures,
                };
                emit(RunEvent::TrialFinished {
                    suite_id: record.suite_id.clone(),
                    eval_id: record.eval_id.clone(),
                    trial_id: record.trial_id.clone(),
                    trial_index: record.trial_index,
                    target_label: record.target.display_label(),
                    passed: record.passed,
                    mean_score: record.mean_score,
                    duration_ms: record.duration.as_millis(),
                    error: record.error.as_ref().map(ToString::to_string),
                });
                record
            }
        }
    }
}
