use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::error::EvalResult;
use crate::eval::EvalContext;
use crate::trial::{AgentTrial, RecordedError, RecordedEvent, RecordedGradingScope};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
type GraderFn<State, Output> = dyn Fn(AgentTrial<Output>, EvalContext<State>) -> BoxFuture<EvalResult<GradeResult>>
    + Send
    + Sync;

pub(crate) fn is_passing_score(score: f32) -> bool {
    (score - 1.0).abs() < f32::EPSILON
}

/// Result returned by a grader.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GradeResult {
    pub score: f32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Value,
}

/// Failure to execute a grader, distinct from a low score.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GraderFailure {
    pub name: String,
    pub error: RecordedError,
}

/// A reusable grading function for an eval trial.
///
/// Most authored code creates a `Grader` through [`predicate`] or [`judge`]
/// instead of constructing it directly.
pub struct Grader<State = (), Output = String> {
    name: String,
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
    /// Returns the stable grader name used in reports and transcripts.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> Grader<State, Output> {
    /// Builds a reusable grader from an async function or closure.
    pub fn new<F, Fut>(name: impl Into<String>, f: F) -> Self
    where
        F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
    {
        Self {
            name: name.into(),
            run: Arc::new(move |trial, ctx| Box::pin(f(trial, ctx))),
        }
    }

    /// Runs the grader against one trial.
    pub async fn grade(
        &self,
        trial: AgentTrial<Output>,
        ctx: EvalContext<State>,
    ) -> EvalResult<GradeResult> {
        (self.run)(trial, ctx).await
    }
}

/// Collection of graders applied to a trial.
///
/// Use `GradingConfig` when an eval needs more than one grading pass.
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

/// Aggregate outcome across all configured graders.
#[derive(Clone, Debug)]
pub struct Grade {
    pub grades: BTreeMap<String, GradeResult>,
    pub passed: bool,
    pub mean_score: f32,
    pub grader_failures: Vec<GraderFailure>,
    pub recorded_events: Vec<RecordedEvent>,
}

impl<State, Output> GradingConfig<State, Output> {
    /// Creates an empty grading configuration.
    pub fn new() -> Self {
        Self {
            graders: Vec::new(),
        }
    }

    /// Adds a pre-built grader.
    pub fn grader(mut self, grader: Grader<State, Output>) -> Self {
        self.graders.push(grader);
        self
    }

    /// Returns the configured graders in execution order.
    pub fn graders(&self) -> &[Grader<State, Output>] {
        &self.graders
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> GradingConfig<State, Output> {
    /// Adds a deterministic grader inline.
    pub fn grade<F, Fut>(self, name: impl Into<String>, f: F) -> Self
    where
        F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
    {
        self.grader(Grader::new(name, f))
    }

    /// Runs all configured graders at eval scope.
    pub async fn run(&self, trial: AgentTrial<Output>, ctx: EvalContext<State>) -> EvalResult<Grade>
    where
        Output: Clone,
    {
        self.run_with_scope(trial, ctx, RecordedGradingScope::Eval)
            .await
    }

    /// Runs all configured graders at the provided grading scope.
    pub async fn run_with_scope(
        &self,
        trial: AgentTrial<Output>,
        ctx: EvalContext<State>,
        scope: RecordedGradingScope,
    ) -> EvalResult<Grade>
    where
        Output: Clone,
    {
        let mut grades = BTreeMap::new();
        let mut grader_failures = Vec::new();
        let mut recorded_events = Vec::new();

        for grader in &self.graders {
            recorded_events.push(RecordedEvent::GraderStarted {
                scope: scope.clone(),
                grader: grader.name().to_string(),
            });
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
                    recorded_events.push(RecordedEvent::GraderCompleted {
                        scope: scope.clone(),
                        grader: grader.name().to_string(),
                        score: grade.score,
                        summary: grade.summary.clone(),
                        evidence: grade.evidence.clone(),
                    });
                    info!(
                        suite_id = %ctx.suite_id,
                        eval_id = %ctx.eval_id,
                        trial_id = %ctx.trial_id,
                        trial_index = ctx.trial_index,
                        target_label = %ctx.target.label,
                        grader = %grader.name(),
                        passed = is_passing_score(grade.score),
                        score = grade.score,
                        "grader completed"
                    );
                    grades.insert(grader.name().to_string(), grade);
                }
                Err(error) => {
                    recorded_events.push(RecordedEvent::GraderFailed {
                        scope: scope.clone(),
                        grader: grader.name().to_string(),
                        error: RecordedError::from_eval_error(&error),
                    });
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
                        error: RecordedError::from_eval_error(&error),
                    });
                }
            }
        }

        let configured_grader_count = grades.len() + grader_failures.len();
        let passed = grader_failures.is_empty()
            && grades.values().all(|grade| is_passing_score(grade.score));
        let mean_score = if configured_grader_count == 0 {
            1.0
        } else {
            grades.values().map(|grade| grade.score).sum::<f32>() / configured_grader_count as f32
        };

        Ok(Grade {
            grades,
            passed,
            mean_score,
            grader_failures,
            recorded_events,
        })
    }
}

impl<State: Send + Sync + 'static, Output: Send + Sync + 'static> From<Grader<State, Output>>
    for GradingConfig<State, Output>
{
    fn from(value: Grader<State, Output>) -> Self {
        GradingConfig::new().grader(value)
    }
}

/// Creates a deterministic grader from ordinary Rust code.
///
/// Use `predicate` when the score should be computed directly from the recorded
/// trial instead of asking another model to judge it.
///
/// ```rust,no_run
/// use evals::{AgentTrial, EvalContext, GradeResult, predicate};
///
/// let grader = predicate("echoes_input", |trial: AgentTrial<String>, _ctx: EvalContext<()>| async move {
///     let reply = trial.final_reply.unwrap_or_default();
///     Ok(GradeResult {
///         score: if reply == "hello" { 1.0 } else { 0.0 },
///         summary: "agent should echo the input".to_string(),
///         evidence: serde_json::json!({ "reply": reply }),
///     })
/// });
/// ```
pub fn predicate<State, Output, F, Fut>(name: impl Into<String>, f: F) -> Grader<State, Output>
where
    State: Send + Sync + 'static,
    Output: Send + Sync + 'static,
    F: Fn(AgentTrial<Output>, EvalContext<State>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
{
    Grader::new(name, f)
}

/// Compatibility alias for [`predicate`].
///
/// Prefer [`predicate`] in new code.
pub use predicate as grade;
