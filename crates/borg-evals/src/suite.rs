use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use borg_agent::Agent;
use borg_llm::LlmRunner;
use borg_llm::error::Error as LlmError;
use borg_llm::provider::anthropic::{Anthropic, AnthropicConfig};
use borg_llm::provider::apple::{Apple, AppleConfig};
use borg_llm::provider::lm_studio::{LmStudio, LmStudioConfig};
use borg_llm::provider::ollama::{Ollama, OllamaConfig};
use borg_llm::provider::openai::{OpenAI, OpenAIConfig};
use borg_llm::provider::openrouter::{OpenRouter, OpenRouterConfig};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::RunEvent;
use crate::config::{ExecutionTarget, ProviderConfigs, RunConfig};
use crate::error::{EvalError, EvalResult};
use crate::eval::{Eval, EvalContext, NoAgent};
use crate::events::emit;
use crate::grade::is_passing_score;
use crate::report::{
    EvalRunReport, IncrementalSuiteWriter, RunManifest, SCHEMA_VERSION, SuiteRunReport,
    TrialRecord, build_summary, now_since_epoch, run_id,
};
use crate::trial::RecordedError;

fn llm_runner_for_target(
    target: &ExecutionTarget,
    provider_configs: &ProviderConfigs,
) -> EvalResult<LlmRunner> {
    let runner = match target.provider.as_str() {
        "default" => LlmRunner::builder().build(),
        "ollama" => {
            let mut config = OllamaConfig::new(target.model.clone());
            if let Some(base_url) = optional_env(&["BORG_LLM_OLLAMA_BASE_URL", "OLLAMA_BASE_URL"])
                .or_else(|| {
                    provider_configs
                        .ollama
                        .as_ref()
                        .map(|config| config.url.clone())
                })
            {
                config = config.with_base_url(base_url);
            }
            LlmRunner::builder()
                .add_provider(Ollama::new(config))
                .build()
        }
        "lm_studio" => {
            let mut config = LmStudioConfig::new(target.model.clone());
            if let Some(base_url) =
                optional_env(&["BORG_LLM_LM_STUDIO_BASE_URL", "LM_STUDIO_BASE_URL"]).or_else(|| {
                    provider_configs
                        .lm_studio
                        .as_ref()
                        .and_then(|config| config.url.clone())
                })
            {
                config = config.with_base_url(base_url);
            }
            if let Some(token) =
                optional_env(&["BORG_LLM_LM_STUDIO_API_TOKEN", "LM_STUDIO_API_TOKEN"]).or_else(
                    || {
                        provider_configs
                            .lm_studio
                            .as_ref()
                            .and_then(|config| config.api_token.clone())
                    },
                )
            {
                config = config.with_api_token(token);
            }
            LlmRunner::builder()
                .add_provider(LmStudio::new(config))
                .build()
        }
        "openai" => {
            let Some(api_key) = optional_env(&[
                "BORG_LLM_OPENAI_API_KEY",
                "OPENAI_API_KEY",
                "BORG_TEST_OPENAI_API_KEY",
            ])
            .or_else(|| {
                provider_configs
                    .openai
                    .as_ref()
                    .and_then(|config| config.api_key.clone())
            }) else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = OpenAIConfig::new(api_key, target.model.clone())
                .map_err(LlmError::OpenAIConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_OPENAI_BASE_URL"]).or_else(|| {
                provider_configs
                    .openai
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            if let Some(org) = optional_env(&["BORG_LLM_OPENAI_ORGANIZATION", "OPENAI_ORG_ID"])
                .or_else(|| {
                    provider_configs
                        .openai
                        .as_ref()
                        .and_then(|config| config.organization.clone())
                })
            {
                config = config.with_organization(org);
            }
            LlmRunner::builder()
                .add_provider(OpenAI::new(config))
                .build()
        }
        "anthropic" => {
            let Some(api_key) = optional_env(&[
                "BORG_LLM_ANTHROPIC_API_KEY",
                "ANTHROPIC_API_KEY",
                "BORG_TEST_ANTHROPIC_API_KEY",
            ])
            .or_else(|| {
                provider_configs
                    .anthropic
                    .as_ref()
                    .and_then(|config| config.api_key.clone())
            }) else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = AnthropicConfig::new(api_key, target.model.clone())
                .map_err(LlmError::AnthropicConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_ANTHROPIC_BASE_URL"]).or_else(|| {
                provider_configs
                    .anthropic
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            if let Some(version) = provider_configs
                .anthropic
                .as_ref()
                .and_then(|config| config.version.clone())
            {
                config = config.with_version(version);
            }
            LlmRunner::builder()
                .add_provider(Anthropic::new(config))
                .build()
        }
        "openrouter" => {
            let Some(api_key) = optional_env(&[
                "BORG_LLM_OPENROUTER_API_KEY",
                "OPENROUTER_API_KEY",
                "BORG_TEST_OPENROUTER_API_KEY",
            ])
            .or_else(|| {
                provider_configs
                    .openrouter
                    .as_ref()
                    .and_then(|config| config.api_key.clone())
            }) else {
                return Ok(LlmRunner::builder().build());
            };
            let mut config = OpenRouterConfig::new(api_key, target.model.clone())
                .map_err(LlmError::OpenRouterConfig)
                .map_err(|error| EvalError::message(error.to_string()))?;
            if let Some(base_url) = optional_env(&["BORG_LLM_OPENROUTER_BASE_URL"]).or_else(|| {
                provider_configs
                    .openrouter
                    .as_ref()
                    .and_then(|config| config.base_url.clone())
            }) {
                config = config.with_base_url(base_url);
            }
            LlmRunner::builder()
                .add_provider(OpenRouter::new(config))
                .build()
        }
        "apple" => LlmRunner::builder()
            .add_provider(Apple::new(AppleConfig::new()))
            .build(),
        provider => {
            return Err(EvalError::message(format!(
                "unsupported eval target provider {:?}",
                provider
            )));
        }
    };

    Ok(runner)
}

fn optional_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
}

fn is_local_provider(provider: &str) -> bool {
    matches!(provider, "ollama" | "lm_studio" | "apple")
}

/// High-level suite classification.
#[derive(Clone, Copy, Debug, Default)]
pub enum SuiteKind {
    #[default]
    Regression,
    Capability,
}

type AgentFactory<State, A> =
    Arc<dyn Fn(EvalContext<State>) -> BoxFuture<EvalResult<A>> + Send + Sync>;
type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'static>>;

/// A collection of related evals that share state and an agent factory.
pub struct Suite<State = (), A = NoAgent>
where
    A: Agent,
{
    id: String,
    kind: SuiteKind,
    trials: usize,
    state: Arc<State>,
    agent_factory: Option<AgentFactory<State, A>>,
    evals: Vec<Eval<State, A>>,
}

impl<State, A> Clone for Suite<State, A>
where
    A: Agent,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            kind: self.kind,
            trials: self.trials,
            state: self.state.clone(),
            agent_factory: self.agent_factory.clone(),
            evals: self.evals.clone(),
        }
    }
}

impl<State, A> std::fmt::Debug for Suite<State, A>
where
    A: Agent,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suite")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("trials", &self.trials)
            .field("evals", &self.evals)
            .finish()
    }
}

pub struct SuiteRunner<'a, State = (), A = NoAgent>
where
    A: Agent,
{
    suite: &'a Suite<State, A>,
    config: RunConfig,
    filter: TargetFilter,
    artifact_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Filter applied while selecting targets and evals to run.
pub struct TargetFilter {
    pub query: Option<String>,
    pub model: Option<String>,
}

impl TargetFilter {
    pub fn matches_target(&self, target: &ExecutionTarget) -> bool {
        self.model.as_deref().is_none_or(|model| {
            target.display_label() == model
                || target.label == model
                || format!("{}/{}", target.provider, target.model) == model
        })
    }
}

#[derive(Debug)]
pub(crate) struct SuitePlan<State = (), A = NoAgent>
where
    A: Agent,
{
    suite: Suite<State, A>,
    config: RunConfig,
    artifact_root: Option<PathBuf>,
}

impl<State, A> SuitePlan<State, A>
where
    A: Agent,
{
    pub(crate) fn suite(&self) -> &Suite<State, A> {
        &self.suite
    }

    pub(crate) fn config(&self) -> &RunConfig {
        &self.config
    }
}

#[async_trait]
trait SuiteExecutor<State, A>: Send + Sync
where
    State: Send + Sync + 'static,
    A: Agent,
{
    async fn run(&self, plan: SuitePlan<State, A>) -> EvalResult<EvalRunReport>;
}

#[derive(Clone, Copy, Debug, Default)]
struct LocalExecutor;

impl Suite<(), NoAgent> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: SuiteKind::Regression,
            trials: 1,
            state: Arc::new(()),
            agent_factory: None,
            evals: Vec::new(),
        }
    }

    pub fn regression_suite(id: impl Into<String>) -> Self {
        Self::new(id)
    }

    pub fn regression(id: impl Into<String>) -> Self {
        Self::regression_suite(id)
    }

    pub fn capability_suite(id: impl Into<String>) -> Self {
        Self::new(id).kind(SuiteKind::Capability)
    }

    pub fn capability(id: impl Into<String>) -> Self {
        Self::capability_suite(id)
    }
}

impl<State, A> Suite<State, A>
where
    A: Agent,
{
    pub fn with_state<NewState>(self, state: NewState) -> Suite<NewState, A> {
        Suite {
            id: self.id,
            kind: self.kind,
            trials: self.trials,
            state: Arc::new(state),
            agent_factory: None,
            evals: Vec::new(),
        }
    }

    pub fn state<NewState>(self, state: NewState) -> Suite<NewState, A> {
        self.with_state(state)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn shared_state(&self) -> &Arc<State> {
        &self.state
    }

    pub fn kind(mut self, kind: SuiteKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn trials(mut self, trials: usize) -> Self {
        self.trials = trials;
        self
    }

    pub fn eval(mut self, eval: Eval<State, A>) -> Self {
        self.evals.push(eval);
        self
    }

    pub fn evals(&self) -> &[Eval<State, A>] {
        &self.evals
    }
}

impl<State, A> Suite<State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    pub fn agent<NewA, F, Fut, E>(self, factory: F) -> Suite<State, NewA>
    where
        NewA: Agent,
        F: Fn(EvalContext<State>) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<NewA, E>> + Send + 'static,
        E: ToString + Send + 'static,
    {
        Suite {
            id: self.id,
            kind: self.kind,
            trials: self.trials,
            state: self.state,
            agent_factory: Some(Arc::new(move |ctx| {
                let factory = factory.clone();
                Box::pin(async move {
                    debug!(
                        suite_id = %ctx.suite_id,
                        eval_id = %ctx.eval_id,
                        trial_id = %ctx.trial_id,
                        trial_index = ctx.trial_index,
                        target_label = %ctx.target.label,
                        "building agent"
                    );
                    factory(ctx)
                        .await
                        .map_err(|error| EvalError::message(error.to_string()))
                })
            })),
            evals: Vec::new(),
        }
    }

    pub async fn run(&self) -> EvalResult<SuiteRunReport> {
        let run_id = run_id();
        run_single_target(
            self,
            run_id,
            &ExecutionTarget::default(),
            &ProviderConfigs::default(),
            self.trials,
            None,
            None,
        )
        .await
    }

    pub fn run_with(&self, config: RunConfig) -> SuiteRunner<'_, State, A> {
        SuiteRunner {
            suite: self,
            config,
            filter: TargetFilter::default(),
            artifact_root: None,
        }
    }
}

impl<'a, State, A> SuiteRunner<'a, State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    pub fn filter(mut self, filter: TargetFilter) -> Self {
        self.filter = filter;
        self
    }

    pub fn persist_to(mut self, root: impl AsRef<Path>) -> Self {
        self.artifact_root = Some(root.as_ref().to_path_buf());
        self
    }

    pub(crate) fn plan(self) -> EvalResult<SuitePlan<State, A>> {
        let mut suite = self.suite.clone();
        let mut config = self.config;
        config
            .targets
            .retain(|target| self.filter.matches_target(target));

        if config.targets.is_empty() {
            return if let Some(model) = self.filter.model {
                Err(EvalError::message(format!(
                    "no eval targets matched model {:?}",
                    model
                )))
            } else {
                Err(EvalError::message("run config has no targets configured"))
            };
        }

        if let Some(query) = self.filter.query.as_deref()
            && !suite.id().contains(query)
        {
            let mut matched_eval_ids = std::collections::BTreeSet::new();
            config.targets.retain(|target| {
                let matching_eval_ids = suite
                    .evals()
                    .iter()
                    .filter_map(|eval| {
                        let search_key = format!("{}::{}::{}", suite.id(), target.label, eval.id());
                        search_key.contains(query).then(|| eval.id().to_string())
                    })
                    .collect::<Vec<_>>();
                let target_has_match = !matching_eval_ids.is_empty();
                matched_eval_ids.extend(matching_eval_ids);
                target_has_match
            });
            suite
                .evals
                .retain(|eval| matched_eval_ids.contains(eval.id()));
        }

        if suite.evals.is_empty() || config.targets.is_empty() {
            return if let Some(query) = self.filter.query {
                Err(EvalError::message(format!(
                    "no suites, models, or evals matched query {:?}",
                    query
                )))
            } else {
                Err(EvalError::message("suite has no evals configured"))
            };
        }

        Ok(SuitePlan {
            suite,
            config,
            artifact_root: self.artifact_root,
        })
    }

    pub async fn run(self) -> EvalResult<EvalRunReport> {
        LocalExecutor.run(self.plan()?).await
    }
}

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
                let _local_permit = if is_local_provider(&target.provider) {
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

async fn run_single_target<State, A>(
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
                execute_trial(
                    run_id,
                    suite_id,
                    state,
                    target,
                    provider_configs,
                    eval,
                    agent_factory,
                    trial_index,
                    timeout,
                )
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

async fn execute_trial<State, A>(
    run_id: String,
    suite_id: String,
    state: Arc<State>,
    target: ExecutionTarget,
    provider_configs: ProviderConfigs,
    eval: Eval<State, A>,
    agent_factory: Option<AgentFactory<State, A>>,
    trial_index: usize,
    timeout: Option<Duration>,
) -> TrialRecord
where
    State: Send + Sync + 'static,
    A: Agent,
{
    let trial_id = Uuid::now_v7().to_string();
    let started_at_wall = now_since_epoch();
    let started_at_instant = Instant::now();
    let llm_runner = match llm_runner_for_target(&target, &provider_configs) {
        Ok(runner) => Arc::new(runner),
        Err(error) => {
            let finished_at_wall = now_since_epoch();
            let record = TrialRecord {
                schema_version: SCHEMA_VERSION,
                trial_id: trial_id.clone(),
                run_id,
                suite_id: suite_id.clone(),
                target: target.clone(),
                eval_id: eval.id().to_string(),
                trial_index,
                started_at: started_at_wall,
                finished_at: finished_at_wall,
                duration: started_at_instant.elapsed(),
                passed: false,
                mean_score: 0.0,
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
        suite_id: suite_id.clone(),
        eval_id: eval.id().to_string(),
        trial_id: trial_id.clone(),
        trial_index,
        target: target.clone(),
        llm_runner,
        state,
    };
    debug!(
        suite_id = %suite_id,
        target_label = %target.label,
        eval_id = %eval.id(),
        trial_id = %trial_id,
        trial_index,
        "starting trial"
    );

    let execution_future = async {
        match agent_factory {
            Some(factory) => match factory(ctx.clone()).await {
                Ok(agent) => {
                    debug!(
                        suite_id = %suite_id,
                        target_label = %target.label,
                        eval_id = %eval.id(),
                        trial_id = %trial_id,
                        trial_index,
                        "agent built"
                    );
                    eval.execute(ctx.clone(), agent).await
                }
                Err(error) => Err(error),
            },
            None => Err(EvalError::message("suite missing agent factory")),
        }
    };

    let execution = match timeout {
        Some(timeout) => match tokio::time::timeout(timeout, execution_future).await {
            Ok(result) => result,
            Err(_) => Err(EvalError::message(format!(
                "trial timed out after {}s",
                timeout.as_secs()
            ))),
        },
        None => execution_future.await,
    };

    match execution {
        Ok(trial) => {
            let mut trial = trial;
            let trajectory_grades = trial.grades.clone();
            let trajectory_grader_failures = trial.grader_failures.clone();
            let outcome = eval
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
                    suite_id = %suite_id,
                    target_label = %target.label,
                    eval_id = %eval.id(),
                    trial_id = %trial_id,
                    trial_index,
                    %error,
                    "trial grading failed"
                );
            } else {
                info!(
                    suite_id = %suite_id,
                    target_label = %target.label,
                    eval_id = %eval.id(),
                    trial_id = %trial_id,
                    trial_index,
                    passed,
                    mean_score,
                    "finished trial"
                );
            }

            let finished_at_wall = now_since_epoch();

            let record = TrialRecord {
                schema_version: SCHEMA_VERSION,
                trial_id,
                run_id,
                suite_id,
                target,
                eval_id: eval.id().to_string(),
                trial_index,
                started_at: started_at_wall,
                finished_at: finished_at_wall,
                duration: started_at_instant.elapsed(),
                passed,
                mean_score,
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
                suite_id = %suite_id,
                target_label = %target.label,
                eval_id = %eval.id(),
                trial_id = %trial_id,
                trial_index,
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
                run_id,
                suite_id,
                target,
                eval_id: eval.id().to_string(),
                trial_index,
                started_at: started_at_wall,
                finished_at: finished_at_wall,
                duration: started_at_instant.elapsed(),
                passed: false,
                mean_score,
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
