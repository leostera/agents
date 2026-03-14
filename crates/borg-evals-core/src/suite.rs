use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::case::{Case, TrialContext};
use crate::config::{ExecutionTarget, RunConfig};
use crate::error::EvalResult;
use crate::report::{
    EvalRunReport, IncrementalSuiteWriter, RunManifest, SCHEMA_VERSION, SuiteRunReport,
    TrialRecord, build_summary, now_since_epoch, run_id,
};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

#[derive(Clone, Copy, Debug, Default)]
pub enum SuiteKind {
    #[default]
    Regression,
    Capability,
}

#[derive(Clone, Debug)]
pub struct Suite {
    id: Arc<str>,
    kind: SuiteKind,
    trials: usize,
    cases: Vec<Case>,
}

pub struct SuiteRunner<'a> {
    suite: &'a Suite,
    config: RunConfig,
    artifact_root: Option<PathBuf>,
}

impl Suite {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Arc::from(id.into()),
            kind: SuiteKind::Regression,
            trials: 1,
            cases: Vec::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(mut self, kind: SuiteKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn trials(mut self, trials: usize) -> Self {
        self.trials = trials;
        self
    }

    pub fn case(mut self, case: Case) -> Self {
        self.cases.push(case);
        self
    }

    pub fn cases(&self) -> &[Case] {
        &self.cases
    }

    pub async fn run(&self) -> EvalResult<SuiteRunReport> {
        let run_id = run_id();
        run_single_target(self, run_id, &ExecutionTarget::default(), self.trials, None).await
    }

    pub fn run_with(&self, config: RunConfig) -> SuiteRunner<'_> {
        SuiteRunner {
            suite: self,
            config,
            artifact_root: None,
        }
    }
}

impl<'a> SuiteRunner<'a> {
    pub fn persist_to(mut self, root: impl AsRef<Path>) -> Self {
        self.artifact_root = Some(root.as_ref().to_path_buf());
        self
    }

    pub async fn run(self) -> EvalResult<EvalRunReport> {
        let started_at = now_since_epoch();
        let run_id = run_id();
        let mut variants = Vec::new();
        let local_target_semaphore = Arc::new(Semaphore::new(1));

        info!(
            suite = %self.suite.id(),
            targets = self.config.targets.len(),
            trials = self.config.trials,
            "starting eval run"
        );

        let mut jobs = JoinSet::new();

        for target in &self.config.targets {
            let target = target.clone();
            let suite = self.suite.clone();
            let run_id = run_id.clone();
            let trials = self.config.trials;
            let local_target_semaphore = local_target_semaphore.clone();
            let artifact_root = self.artifact_root.clone();
            jobs.spawn(async move {
                let _local_permit = if target.provider == "ollama" {
                    Some(
                        local_target_semaphore
                            .acquire_owned()
                            .await
                            .expect("local target semaphore permit"),
                    )
                } else {
                    None
                };
                run_single_target(&suite, run_id, &target, trials, artifact_root.as_deref()).await
            });
        }

        while let Some(result) = jobs.join_next().await {
            variants.push(result.expect("target task panicked")?);
        }

        variants.sort_by(|left, right| left.suite.target.label.cmp(&right.suite.target.label));

        let finished_at = now_since_epoch();
        let manifest = RunManifest {
            schema_version: SCHEMA_VERSION,
            run_id,
            started_at,
            finished_at,
            suites: vec![self.suite.id().to_string()],
            targets: self.config.targets,
        };

        info!(
            suite = %self.suite.id(),
            variants = variants.len(),
            "finished eval run"
        );

        Ok(EvalRunReport { manifest, variants })
    }
}

async fn run_single_target(
    suite: &Suite,
    run_id: String,
    target: &ExecutionTarget,
    default_trials: usize,
    artifact_root: Option<&Path>,
) -> EvalResult<SuiteRunReport> {
    let started_at = now_since_epoch();
    let mut trial_records = Vec::new();

    info!(
        suite = %suite.id(),
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

    for case in suite.cases() {
        let trial_count = case.configured_trials().unwrap_or(default_trials);
        info!(
            suite = %suite.id(),
            target_label = %target.label,
            case = %case.id(),
            trials = trial_count,
            "starting case"
        );
        for trial_index in 0..trial_count {
            let semaphore = semaphore.clone();
            let suite_id = suite.id().to_string();
            let target = target.clone();
            let case = case.clone();
            let run_id = run_id.clone();

            jobs.spawn(async move {
                let _permit = semaphore.acquire_owned().await.expect("semaphore permit");
                execute_trial(run_id, suite_id, target, case, trial_index).await
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
        left.case_id
            .cmp(&right.case_id)
            .then(left.trial_index.cmp(&right.trial_index))
    });

    let finished_at = now_since_epoch();
    let summary = build_summary(suite, &run_id, target, &trial_records);
    info!(
        suite = %suite.id(),
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
    };

    let report = SuiteRunReport {
        manifest,
        suite: summary,
        trials: trial_records,
    };

    if let Some(writer) = incremental_writer.as_mut() {
        writer.finish(&report)?;
    }

    Ok(report)
}

async fn execute_trial(
    run_id: String,
    suite_id: String,
    target: ExecutionTarget,
    case: Case,
    trial_index: usize,
) -> TrialRecord {
    let started_at_wall = now_since_epoch();
    let started_at_instant = Instant::now();
    let ctx = TrialContext {
        suite_id: suite_id.clone(),
        case_id: case.id().to_string(),
        trial_index,
        target: target.clone(),
    };
    debug!(
        suite = %suite_id,
        target_label = %target.label,
        case = %case.id(),
        trial_index,
        "starting trial"
    );

    match case.execute(ctx).await {
        Ok(trial) => {
            let trial = Arc::new(trial);
            let mut grades = Vec::new();
            let mut grade_error = None;

            for grader in case.graders() {
                match grader.grade(trial.clone()).await {
                    Ok(grade) => grades.push(grade),
                    Err(error) => {
                        grade_error = Some(error.to_string());
                        break;
                    }
                }
            }

            let (passed, mean_score, error) = if let Some(error) = grade_error {
                (false, 0.0, Some(error))
            } else {
                let passed = grades.iter().all(|grade| grade.passed);
                let mean_score = if grades.is_empty() {
                    1.0
                } else {
                    grades.iter().map(|grade| grade.score).sum::<f32>() / grades.len() as f32
                };
                (passed, mean_score, None)
            };

            if let Some(error) = &error {
                warn!(
                    suite = %suite_id,
                    target_label = %target.label,
                    case = %case.id(),
                    trial_index,
                    %error,
                    "trial grading failed"
                );
            } else {
                info!(
                    suite = %suite_id,
                    target_label = %target.label,
                    case = %case.id(),
                    trial_index,
                    passed,
                    mean_score,
                    "finished trial"
                );
            }

            let finished_at_wall = now_since_epoch();

            TrialRecord {
                schema_version: SCHEMA_VERSION,
                run_id,
                suite_id,
                target,
                case_id: case.id().to_string(),
                trial_index,
                started_at: started_at_wall,
                finished_at: finished_at_wall,
                duration: started_at_instant.elapsed(),
                passed,
                mean_score,
                trial: Some((*trial).clone()),
                error,
                grades,
            }
        }
        Err(error) => {
            error!(
                suite = %suite_id,
                target_label = %target.label,
                case = %case.id(),
                trial_index,
                error = %error,
                "trial execution failed"
            );
            let finished_at_wall = now_since_epoch();
            TrialRecord {
                schema_version: SCHEMA_VERSION,
                run_id,
                suite_id,
                target,
                case_id: case.id().to_string(),
                trial_index,
                started_at: started_at_wall,
                finished_at: finished_at_wall,
                duration: started_at_instant.elapsed(),
                passed: false,
                mean_score: 0.0,
                trial: None,
                error: Some(error.to_string()),
                grades: Vec::new(),
            }
        }
    }
}
