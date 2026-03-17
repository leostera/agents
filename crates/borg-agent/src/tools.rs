use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEnvelope<C> {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub call: C,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolExecutionResult<T> {
    Ok { data: T },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultEnvelope<T> {
    pub call_id: String,
    #[serde(flatten)]
    pub result: ToolExecutionResult<T>,
}

#[async_trait]
pub trait ToolRunner<C, T>: Send + Sync {
    async fn run(&self, call: ToolCallEnvelope<C>) -> AgentResult<ToolResultEnvelope<T>>;
}

pub struct NoToolRunner;

#[async_trait]
impl<C, R> ToolRunner<C, R> for NoToolRunner
where
    C: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    async fn run(&self, call: ToolCallEnvelope<C>) -> AgentResult<ToolResultEnvelope<R>> {
        Err(AgentError::ToolExecution {
            reason: format!("unexpected tool call with id {}", call.call_id),
        })
    }
}

type BoxedToolFuture<T> = Pin<Box<dyn Future<Output = AgentResult<ToolResultEnvelope<T>>> + Send>>;
type ToolCallback<C, T> = Arc<dyn Fn(ToolCallEnvelope<C>) -> BoxedToolFuture<T> + Send + Sync>;

pub struct CallbackToolRunner<C, T> {
    callback: ToolCallback<C, T>,
}

impl<C, T> CallbackToolRunner<C, T> {
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(ToolCallEnvelope<C>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AgentResult<ToolResultEnvelope<T>>> + Send + 'static,
    {
        Self {
            callback: Arc::new(move |call| Box::pin(callback(call))),
        }
    }
}

#[async_trait]
impl<C, T> ToolRunner<C, T> for CallbackToolRunner<C, T>
where
    C: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    async fn run(&self, call: ToolCallEnvelope<C>) -> AgentResult<ToolResultEnvelope<T>> {
        (self.callback)(call).await
    }
}
