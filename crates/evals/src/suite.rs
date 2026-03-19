mod executor;
mod llm;
mod planning;
mod target;
mod trial;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agents::agent::Agent;
use tracing::debug;

use crate::RunEvent;
use crate::config::{ExecutionTarget, ProviderConfigs, RunConfig};
use crate::error::{EvalError, EvalResult};
use crate::eval::{Eval, EvalContext, NoAgent};
use crate::report::{
    EvalRunReport, RunManifest, SCHEMA_VERSION, SuiteRunReport, now_since_epoch, run_id,
};

use self::executor::{LocalExecutor, SuiteExecutor};
use self::target::run_single_target;

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

    pub async fn run(self) -> EvalResult<EvalRunReport> {
        LocalExecutor.run(self.plan()?).await
    }
}
