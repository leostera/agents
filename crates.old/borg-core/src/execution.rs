use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult<TResult> {
    pub stdout: String,
    pub stderr: String,
    pub result: TResult,
    pub duration: Duration,
}
