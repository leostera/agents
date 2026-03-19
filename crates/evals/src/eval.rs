use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use agents::agent::{Agent, AgentError, AgentEvent, AgentInput, AgentRunInput, AgentRunOutput};
use agents::llm::LlmRunner;
use tracing::debug;

use crate::config::ExecutionTarget;
use crate::error::EvalResult;
use crate::grade::GradingConfig;
use crate::trial::AgentTrial;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// Marker agent used before a suite installs its real agent factory.
#[derive(Clone, Debug)]
pub struct NoAgent;

impl Agent for NoAgent {
    type Input = serde_json::Value;
    type ToolCall = serde_json::Value;
    type ToolResult = serde_json::Value;
    type Output = String;

    async fn send(&mut self, _input: AgentInput<Self::Input>) -> Result<(), AgentError> {
        Err(AgentError::Internal {
            message: "no agent factory configured".to_string(),
        })
    }

    async fn next(
        &mut self,
    ) -> Result<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>, AgentError>
    {
        Err(AgentError::Internal {
            message: "no agent factory configured".to_string(),
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
        Err(AgentError::Internal {
            message: "no agent factory configured".to_string(),
        })
    }
}

/// Per-trial context passed into suite agent factories, eval runners, and graders.
pub struct EvalContext<State = ()> {
    pub suite_id: String,
    pub eval_id: String,
    pub trial_id: String,
    pub trial_index: usize,
    pub target: ExecutionTarget,
    pub llm_runner: Arc<LlmRunner>,
    pub state: Arc<State>,
}

impl<State> Clone for EvalContext<State> {
    fn clone(&self) -> Self {
        Self {
            suite_id: self.suite_id.clone(),
            eval_id: self.eval_id.clone(),
            trial_id: self.trial_id.clone(),
            trial_index: self.trial_index,
            target: self.target.clone(),
            llm_runner: self.llm_runner.clone(),
            state: self.state.clone(),
        }
    }
}

impl<State> std::fmt::Debug for EvalContext<State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalContext")
            .field("suite_id", &self.suite_id)
            .field("eval_id", &self.eval_id)
            .field("trial_id", &self.trial_id)
            .field("trial_index", &self.trial_index)
            .field("target", &self.target)
            .finish()
    }
}

impl<State> EvalContext<State> {
    /// Returns the target being evaluated.
    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }

    /// Returns the shared suite state.
    pub fn state(&self) -> &Arc<State> {
        &self.state
    }

    /// Returns the shared runner for the current target.
    pub fn llm_runner(&self) -> Arc<LlmRunner> {
        self.llm_runner.clone()
    }
}

type EvalRunner<State, A> = Arc<
    dyn Fn(EvalContext<State>, A) -> BoxFuture<EvalResult<AgentTrial<<A as Agent>::Output>>>
        + Send
        + Sync,
>;

/// A single eval scenario for an agent type.
///
/// An `Eval` combines an identifier, optional tags and trial overrides, a
/// runner closure that produces an [`AgentTrial`], and one or more graders.
pub struct Eval<State = (), Agent = NoAgent>
where
    Agent: agents::agent::Agent,
{
    id: String,
    tags: Vec<String>,
    trials: Option<usize>,
    timeout: Option<Duration>,
    run: Option<EvalRunner<State, Agent>>,
    grading: GradingConfig<State, Agent::Output>,
}

impl<State, A> Clone for Eval<State, A>
where
    A: Agent,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            tags: self.tags.clone(),
            trials: self.trials,
            timeout: self.timeout,
            run: self.run.clone(),
            grading: self.grading.clone(),
        }
    }
}

impl<State, A> std::fmt::Debug for Eval<State, A>
where
    A: Agent,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eval")
            .field("id", &self.id)
            .field("tags", &self.tags)
            .field("trials", &self.trials)
            .field("timeout", &self.timeout)
            .field("grading", &self.grading)
            .finish()
    }
}

impl<State, A> Eval<State, A>
where
    A: Agent,
{
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            tags: Vec::new(),
            trials: None,
            timeout: None,
            run: None,
            grading: GradingConfig::new(),
        }
    }

    /// Returns the eval identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the configured tag list.
    pub fn tag_list(&self) -> &[String] {
        &self.tags
    }

    /// Returns the explicitly configured trial count, if any.
    pub fn configured_trials(&self) -> Option<usize> {
        self.trials
    }

    /// Returns the explicitly configured timeout for this eval, if any.
    pub fn configured_timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Adds a single tag.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Adds many tags.
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags.extend(tags.into_iter().map(Into::into));
        self
    }

    /// Overrides the number of trials for this eval.
    pub fn trials(mut self, trials: usize) -> Self {
        self.trials = Some(trials);
        self
    }

    /// Overrides the timeout for this eval.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Replaces the grading configuration for this eval.
    pub fn grading<G>(mut self, grading: G) -> Self
    where
        G: Into<GradingConfig<State, A::Output>>,
    {
        self.grading = grading.into();
        self
    }

    /// Returns the configured graders.
    pub fn graders(&self) -> &[crate::grade::Grader<State, A::Output>] {
        self.grading.graders()
    }

    /// Returns the grading configuration.
    pub fn grading_config(&self) -> &GradingConfig<State, A::Output> {
        &self.grading
    }
}

impl<State, A> Eval<State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    pub fn run<F, Fut>(mut self, run: F) -> Self
    where
        F: Fn(EvalContext<State>, A) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<AgentTrial<A::Output>>> + 'static,
    {
        self.run = Some(Arc::new(move |ctx, agent| Box::pin(run(ctx, agent))));
        self
    }

    pub async fn execute(
        &self,
        ctx: EvalContext<State>,
        agent: A,
    ) -> EvalResult<AgentTrial<A::Output>> {
        debug!(
            suite_id = %ctx.suite_id,
            eval_id = %ctx.eval_id,
            trial_id = %ctx.trial_id,
            trial_index = ctx.trial_index,
            target_label = %ctx.target.label,
            "executing eval"
        );
        let run = self.run.as_ref().expect("eval missing run function");
        let trial = run(ctx.clone(), agent).await?;
        Ok(trial)
    }
}
