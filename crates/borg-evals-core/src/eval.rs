use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use borg_agent::{Agent, AgentRunInput, AgentRunOutput};
use borg_llm::completion::InputItem;
use borg_llm::tools::TypedTool;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::{debug, info};

use crate::config::ExecutionTarget;
use crate::error::{EvalError, EvalResult};
use crate::grade::GradingConfig;
use crate::trial::AgentTrial;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone, Debug)]
pub struct NoAgent;

#[async_trait]
impl EvalAgent for NoAgent {
    type Input = serde_json::Value;
    type ToolCall = serde_json::Value;
    type ToolResult = serde_json::Value;
    type Output = String;

    async fn run(
        self,
    ) -> EvalResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        Err(EvalError::message("no agent factory configured"))
    }
}

#[async_trait]
pub trait EvalAgent: Send + 'static {
    type Input: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type ToolCall: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type ToolResult: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    type Output: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    async fn run(
        self,
    ) -> EvalResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )>;
}

#[async_trait]
impl<MI, TC, TR, MO> EvalAgent for Agent<MI, TC, TR, MO>
where
    MI: Into<InputItem> + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    TC: TypedTool + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    TR: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    MO: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    type Input = MI;
    type ToolCall = TC;
    type ToolResult = TR;
    type Output = MO;

    async fn run(
        self,
    ) -> EvalResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        Agent::run(self)
            .await
            .map_err(|error| EvalError::message(error.to_string()))
    }
}

pub struct EvalContext<State = ()> {
    pub suite_id: String,
    pub eval_id: String,
    pub trial_id: String,
    pub trial_index: usize,
    pub target: ExecutionTarget,
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
    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }

    pub fn state(&self) -> &Arc<State> {
        &self.state
    }
}

type EvalRunner<State, A> = Arc<
    dyn Fn(EvalContext<State>, A) -> BoxFuture<EvalResult<AgentTrial<<A as EvalAgent>::Output>>>
        + Send
        + Sync,
>;

pub struct Eval<State = (), Agent = NoAgent>
where
    Agent: EvalAgent,
{
    id: String,
    tags: Vec<String>,
    trials: Option<usize>,
    run: Option<EvalRunner<State, Agent>>,
    grading: GradingConfig<State, Agent::Output>,
}

impl<State, A> Clone for Eval<State, A>
where
    A: EvalAgent,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            tags: self.tags.clone(),
            trials: self.trials,
            run: self.run.clone(),
            grading: self.grading.clone(),
        }
    }
}

impl<State, A> std::fmt::Debug for Eval<State, A>
where
    A: EvalAgent,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eval")
            .field("id", &self.id)
            .field("tags", &self.tags)
            .field("trials", &self.trials)
            .field("grading", &self.grading)
            .finish()
    }
}

impl<State, A> Eval<State, A>
where
    A: EvalAgent,
{
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            tags: Vec::new(),
            trials: None,
            run: None,
            grading: GradingConfig::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn tag_list(&self) -> &[String] {
        &self.tags
    }

    pub fn configured_trials(&self) -> Option<usize> {
        self.trials
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags.extend(tags.into_iter().map(Into::into));
        self
    }

    pub fn trials(mut self, trials: usize) -> Self {
        self.trials = Some(trials);
        self
    }

    pub fn grading<G>(mut self, grading: G) -> Self
    where
        G: crate::grade::IntoGradingConfig<State, A::Output>,
    {
        self.grading = grading.into_grading_config();
        self
    }

    pub fn graders(&self) -> &[crate::grade::Grader<State, A::Output>] {
        self.grading.graders()
    }

    pub fn grading_config(&self) -> &GradingConfig<State, A::Output> {
        &self.grading
    }
}

impl<State, A> Eval<State, A>
where
    State: Send + Sync + 'static,
    A: EvalAgent,
{
    pub fn run<F, Fut>(mut self, run: F) -> Self
    where
        F: Fn(EvalContext<State>, A) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<AgentTrial<A::Output>>> + Send + 'static,
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
        info!(
            suite_id = %ctx.suite_id,
            eval_id = %ctx.eval_id,
            trial_id = %ctx.trial_id,
            trial_index = ctx.trial_index,
            target_label = %ctx.target.label,
            "eval completed"
        );
        Ok(trial)
    }
}
