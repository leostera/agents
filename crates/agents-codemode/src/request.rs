use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request handled by [`CodeMode`](crate::CodeMode).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    RunCode(RunCode),
    SearchCode(SearchCode),
}

/// Response returned by [`CodeMode`](crate::CodeMode).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    RunCode(RunCodeResult),
    SearchCode(SearchCodeResult),
}

/// Executes JavaScript code inside the codemode engine.
///
/// `code` is expected to be an async zero-argument closure:
/// `async () => { ... }`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCode {
    pub code: String,
    #[serde(default)]
    pub imports: Vec<String>,
}

/// Searches available packages/code exposed by package providers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCode {
    pub query: String,
    pub limit: Option<usize>,
}

/// Structured result of a [`RunCode`] request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunCodeResult {
    pub value: Value,
    pub duration: Duration,
}

/// Structured result of a [`SearchCode`] request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCodeResult {
    pub matches: Vec<PackageMatch>,
}

/// A package hit returned from [`SearchCode`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMatch {
    pub name: String,
    pub snippet: Option<String>,
}
