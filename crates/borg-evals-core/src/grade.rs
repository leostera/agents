use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::EvalResult;
use crate::trial::AgentTrial;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GradeResult {
    pub name: String,
    pub passed: bool,
    pub score: f32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Value,
}

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

#[derive(Clone)]
pub struct Grader {
    name: Arc<str>,
    run: Arc<dyn Fn(Arc<AgentTrial>) -> BoxFuture<EvalResult<GradeResult>> + Send + Sync>,
}

impl std::fmt::Debug for Grader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grader").field("name", &self.name).finish()
    }
}

impl Grader {
    pub fn new<F, Fut>(name: impl Into<String>, f: F) -> Self
    where
        F: Fn(Arc<AgentTrial>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
    {
        Self {
            name: Arc::from(name.into()),
            run: Arc::new(move |trial| Box::pin(f(trial))),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn grade(&self, trial: Arc<AgentTrial>) -> EvalResult<GradeResult> {
        (self.run)(trial).await
    }
}

pub fn grade<F, Fut>(name: impl Into<String>, f: F) -> Grader
where
    F: Fn(Arc<AgentTrial>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = EvalResult<GradeResult>> + Send + 'static,
{
    Grader::new(name, f)
}
