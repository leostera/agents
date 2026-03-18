use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;

/// Async host function exposed to JavaScript by [`CodeMode`](crate::CodeMode).
#[async_trait]
pub trait NativeFunction: Send + Sync + 'static {
    async fn call(&self, args: Value) -> Result<Value>;
}

#[async_trait]
impl<F, Fut> NativeFunction for F
where
    F: Fn(Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value>> + Send + 'static,
{
    async fn call(&self, args: Value) -> Result<Value> {
        (self)(args).await
    }
}

/// Named host functions exposed to JavaScript.
#[derive(Clone, Default)]
pub struct NativeFunctionRegistry {
    functions: BTreeMap<String, Arc<dyn NativeFunction>>,
}

impl NativeFunctionRegistry {
    /// Adds one native function under the provided name.
    pub fn add_function<F>(mut self, name: impl Into<String>, function: F) -> Self
    where
        F: NativeFunction,
    {
        self.functions.insert(name.into(), Arc::new(function));
        self
    }

    pub(crate) fn merge_from(&mut self, other: Self) {
        self.functions.extend(other.functions);
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }

    pub(crate) async fn call(&self, name: &str, args: Value) -> Result<Value> {
        let Some(function) = self.functions.get(name) else {
            return Err(anyhow!("native function not found: {name}"));
        };
        function.call(args).await
    }
}
