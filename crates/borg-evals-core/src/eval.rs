use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::ExecutionTarget;
use crate::error::EvalResult;
use crate::grade::Grader;
use crate::trial::AgentTrial;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone, Debug)]
pub struct EvalContext {
    pub suite_id: String,
    pub eval_id: String,
    pub trial_index: usize,
    pub target: ExecutionTarget,
}

impl EvalContext {
    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }
}

#[derive(Clone)]
pub struct Eval {
    id: Arc<str>,
    tags: Vec<String>,
    trials: Option<usize>,
    run: Arc<dyn Fn(EvalContext) -> BoxFuture<EvalResult<AgentTrial>> + Send + Sync>,
    graders: Vec<Grader>,
}

impl std::fmt::Debug for Eval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Eval")
            .field("id", &self.id)
            .field("tags", &self.tags)
            .field("trials", &self.trials)
            .field("graders", &self.graders)
            .finish()
    }
}

impl Eval {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Arc::from(id.into()),
            tags: Vec::new(),
            trials: None,
            run: Arc::new(|_| Box::pin(async { Ok(AgentTrial::new("")) })),
            graders: Vec::new(),
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

    pub fn run<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(EvalContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<AgentTrial>> + Send + 'static,
    {
        self.run = Arc::new(move |ctx| Box::pin(f(ctx)));
        self
    }

    pub fn grade(mut self, grader: Grader) -> Self {
        self.graders.push(grader);
        self
    }

    pub async fn execute(&self, ctx: EvalContext) -> EvalResult<AgentTrial> {
        (self.run)(ctx).await
    }

    pub fn graders(&self) -> &[Grader] {
        &self.graders
    }
}
