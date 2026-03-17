use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::error::EvalResult;
use crate::eval::EvalContext;
use crate::trial::AgentTrial;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
type GraderFn<State, Output> = dyn Fn(AgentTrial<Output>, EvalContext<State>) -> BoxFuture<EvalResult<GradeResult>>
    + Send
    + Sync;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GradeResult {
    // TODO(@leostera): grade results do not need names!
    pub name: String,
    pub passed: bool,
    pub score: f32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Value,
}

// TODO(@leostera): remove these helpers, and use a builder-style api:
//    GradeResult::builder().score(0.5).summary("...").evidence(json).build()
//    GradeResult::builder().score(0.5).summary("...").evidence(json).build()
//
impl GradeResult {
    pub fn pass(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            score: 1.0,
            summary: summary.into(),
            evidence: Value::Null,
        }
    }

    pub fn fail(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            score: 0.0,
            summary: summary.into(),
            evidence: Value::Null,
        }
    }

    pub fn pass_if(
        name: impl Into<String>,
        passed: bool,
        summary: impl Into<String>,
        evidence: Value,
    ) -> Self {
        Self {
            name: name.into(),
            passed,
            score: if passed { 1.0 } else { 0.0 },
            summary: summary.into(),
            evidence,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GraderFailure {
    pub name: String,
    pub error: String,
}

pub struct Grader<State = (), Output = String> {
    name: Arc<str>,
    run: Arc<GraderFn<State, Output>>,
}

impl<State, Output> Clone for Grader<State, Output> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            run: self.run.clone(),
        }
    }
}

impl<State, Output> std::fmt::Debug for Grader<State, Output> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grader").field("name", &self.name).finish()
    }
}

impl<State, Output> Grader<State, Output> {
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> Grader<State, Output> {
    pub fn new<F, Fut>(name: impl Into<String>, f: F) -> Self
    where
        F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
    {
        Self {
            name: Arc::from(name.into()),
            run: Arc::new(move |trial, ctx| Box::pin(f(trial, ctx))),
        }
    }

    pub async fn grade(
        &self,
        trial: AgentTrial<Output>,
        ctx: EvalContext<State>,
    ) -> EvalResult<GradeResult> {
        (self.run)(trial, ctx).await
    }
}

pub struct GradingConfig<State = (), Output = String> {
    graders: Vec<Grader<State, Output>>,
}

impl<State, Output> Clone for GradingConfig<State, Output> {
    fn clone(&self) -> Self {
        Self {
            graders: self.graders.clone(),
        }
    }
}

impl<State, Output> std::fmt::Debug for GradingConfig<State, Output> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GradingConfig")
            .field("graders", &self.graders)
            .finish()
    }
}

impl<State, Output> Default for GradingConfig<State, Output> {
    fn default() -> Self {
        Self {
            graders: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grade {
    pub grades: Vec<GradeResult>,
    pub passed: bool,
    pub mean_score: f32,
    pub grader_failures: Vec<GraderFailure>,
}

impl<State, Output> GradingConfig<State, Output> {
    pub fn new() -> Self {
        Self {
            graders: Vec::new(),
        }
    }

    pub fn grader(mut self, grader: Grader<State, Output>) -> Self {
        self.graders.push(grader);
        self
    }

    pub fn graders(&self) -> &[Grader<State, Output>] {
        &self.graders
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> GradingConfig<State, Output> {
    pub fn grade<F, Fut>(self, name: impl Into<String>, f: F) -> Self
    where
        F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
    {
        self.grader(Grader::new(name, f))
    }

    pub async fn run(&self, trial: AgentTrial<Output>, ctx: EvalContext<State>) -> EvalResult<Grade>
    where
        Output: Clone,
    {
        let mut grades = Vec::with_capacity(self.graders.len());
        let mut grader_failures = Vec::new();

        for grader in &self.graders {
            debug!(
                suite_id = %ctx.suite_id,
                eval_id = %ctx.eval_id,
                trial_id = %ctx.trial_id,
                trial_index = ctx.trial_index,
                target_label = %ctx.target.label,
                grader = %grader.name(),
                "running grader"
            );
            match grader.grade(trial.clone(), ctx.clone()).await {
                Ok(grade) => {
                    info!(
                        suite_id = %ctx.suite_id,
                        eval_id = %ctx.eval_id,
                        trial_id = %ctx.trial_id,
                        trial_index = ctx.trial_index,
                        target_label = %ctx.target.label,
                        grader = %grader.name(),
                        passed = grade.passed,
                        score = grade.score,
                        "grader completed"
                    );
                    grades.push(grade);
                }
                Err(error) => {
                    warn!(
                        suite_id = %ctx.suite_id,
                        eval_id = %ctx.eval_id,
                        trial_id = %ctx.trial_id,
                        trial_index = ctx.trial_index,
                        target_label = %ctx.target.label,
                        grader = %grader.name(),
                        error = %error,
                        "grader failed"
                    );
                    grader_failures.push(GraderFailure {
                        name: grader.name().to_string(),
                        error: error.to_string(),
                    });
                }
            }
        }

        let configured_grader_count = grades.len() + grader_failures.len();
        let passed = grader_failures.is_empty() && grades.iter().all(|grade| grade.passed);
        let mean_score = if configured_grader_count == 0 {
            1.0
        } else {
            grades.iter().map(|grade| grade.score).sum::<f32>() / configured_grader_count as f32
        };

        Ok(Grade {
            grades,
            passed,
            mean_score,
            grader_failures,
        })
    }
}

pub trait IntoGradingConfig<State, Output> {
    fn into_grading_config(self) -> GradingConfig<State, Output>;
}

impl<State, Output> IntoGradingConfig<State, Output> for GradingConfig<State, Output> {
    fn into_grading_config(self) -> GradingConfig<State, Output> {
        self
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> IntoGradingConfig<State, Output>
    for Grader<State, Output>
{
    fn into_grading_config(self) -> GradingConfig<State, Output> {
        GradingConfig::new().grader(self)
    }
}

pub fn grade<State, Output, F, Fut>(name: impl Into<String>, f: F) -> Grader<State, Output>
where
    State: Send + Sync + 'static,
    Output: Send + Sync + 'static,
    F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
{
    Grader::new(name, f)
}
