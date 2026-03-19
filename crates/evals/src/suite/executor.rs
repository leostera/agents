use std::sync::Arc;

use agents::agent::Agent;
use async_trait::async_trait;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::info;

use super::planning::SuitePlan;
use super::target::run_single_target;
use crate::RunEvent;
use crate::error::EvalResult;
use crate::events::emit;
use crate::report::{EvalRunReport, RunManifest, SCHEMA_VERSION, now_since_epoch, run_id};

#[async_trait]
pub(super) trait SuiteExecutor<State, A>: Send + Sync
where
    State: Send + Sync + 'static,
    A: Agent,
{
    async fn run(&self, plan: SuitePlan<State, A>) -> EvalResult<EvalRunReport>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LocalExecutor;

#[async_trait]
impl<State, A> SuiteExecutor<State, A> for LocalExecutor
where
    State: Send + Sync + 'static,
    A: Agent,
{
    async fn run(&self, plan: SuitePlan<State, A>) -> EvalResult<EvalRunReport> {
        let SuitePlan {
            suite,
            config,
            artifact_root,
        } = plan;
        let started_at = now_since_epoch();
        let run_id = run_id();
        let mut variants = Vec::new();
        let local_target_semaphore = Arc::new(Semaphore::new(1));

        emit(RunEvent::RunStarted {
            suite_count: 1,
            targets: config
                .targets
                .iter()
                .map(|target| target.display_label())
                .collect(),
            trials: config.trials,
            output_dir: artifact_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| ".evals".to_string()),
        });

        info!(
            suite_id = %suite.id(),
            targets = config.targets.len(),
            trials = config.trials,
            "starting eval run"
        );

        let mut jobs = JoinSet::new();

        for target in &config.targets {
            let target = target.clone();
            let suite = suite.clone();
            let run_id = run_id.clone();
            let trials = config.trials;
            let provider_configs = config.provider.clone();
            let local_target_semaphore = local_target_semaphore.clone();
            let artifact_root = artifact_root.clone();
            jobs.spawn(async move {
                let _local_permit = if target.is_local() {
                    Some(
                        local_target_semaphore
                            .acquire_owned()
                            .await
                            .expect("local target semaphore permit"),
                    )
                } else {
                    None
                };
                run_single_target(
                    &suite,
                    run_id,
                    &target,
                    &provider_configs,
                    trials,
                    config.timeout,
                    artifact_root.as_deref(),
                )
                .await
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
            suites: vec![suite.id().to_string()],
            targets: config.targets,
            files: Vec::new(),
        };

        info!(
            suite_id = %suite.id(),
            variants = variants.len(),
            "finished eval run"
        );

        emit(RunEvent::RunFinished {
            suite_count: 1,
            variant_count: variants.len(),
        });

        Ok(EvalRunReport { manifest, variants })
    }
}
