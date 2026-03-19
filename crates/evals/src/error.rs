use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::trial::AgentTrial;

/// Result type used across the eval runner and artifact pipeline.
pub type EvalResult<T> = Result<T, EvalError>;

/// Structured errors produced by the eval system.
#[derive(Debug, Error, Clone, Serialize, Deserialize, JsonSchema)]
pub enum EvalError {
    #[error("io error: {message}")]
    Io { message: String },

    #[error("serialization error: {message}")]
    Serde { message: String },

    #[error(
        "eval failed: no eval targets configured; add at least one [[evals.targets]] entry to evals.toml"
    )]
    NoTargetsConfigured,

    #[error("eval failed: no eval targets matched model {model:?}")]
    NoTargetsMatchedModel { model: String },

    #[error("eval failed: no suites, models, or evals matched query {query:?}")]
    NoMatchesForQuery { query: String },

    #[error("eval failed: suite {suite_id:?} has no evals configured")]
    SuiteHasNoEvals { suite_id: String },

    #[error("eval failed: no eval suites discovered in the workspace")]
    NoSuitesDiscovered,

    #[error("eval failed: no eval suites matched the selected filters")]
    NoSuitesMatchedFilters,

    #[error("eval failed: trial timed out after {}", format_duration_ms(*duration_ms))]
    TrialTimedOut { duration_ms: u64 },

    #[error("eval failed: {message}")]
    Message { message: String },

    #[error("eval failed: {message}")]
    MessageWithTrial {
        message: String,
        trial: Box<serde_json::Value>,
    },
}

impl EvalError {
    /// Builds a message-only eval error.
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            message: message.into(),
        }
    }

    /// Builds an eval error that preserves a partial trial snapshot.
    pub fn message_with_trial<Output>(message: impl Into<String>, trial: AgentTrial<Output>) -> Self
    where
        Output: serde::Serialize,
    {
        Self::MessageWithTrial {
            message: message.into(),
            trial: Box::new(serde_json::to_value(trial).expect("serialize partial trial")),
        }
    }

    /// Builds a structured timeout error from a runtime duration.
    pub fn trial_timed_out(duration: std::time::Duration) -> Self {
        Self::TrialTimedOut {
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
        }
    }

    /// Builds a structured planner error for an unmatched model filter.
    pub fn no_targets_matched_model(model: impl Into<String>) -> Self {
        Self::NoTargetsMatchedModel {
            model: model.into(),
        }
    }

    /// Builds a structured planner error for an unmatched query filter.
    pub fn no_matches_for_query(query: impl Into<String>) -> Self {
        Self::NoMatchesForQuery {
            query: query.into(),
        }
    }

    /// Builds a structured planner error when a suite has no remaining evals.
    pub fn suite_has_no_evals(suite_id: impl Into<String>) -> Self {
        Self::SuiteHasNoEvals {
            suite_id: suite_id.into(),
        }
    }

    /// Returns the serialized partial trial, if one was attached.
    pub fn partial_trial_json(&self) -> Option<&serde_json::Value> {
        match self {
            Self::MessageWithTrial { trial, .. } => Some(trial.as_ref()),
            Self::Io { .. }
            | Self::Serde { .. }
            | Self::NoTargetsConfigured
            | Self::NoTargetsMatchedModel { .. }
            | Self::NoMatchesForQuery { .. }
            | Self::SuiteHasNoEvals { .. }
            | Self::NoSuitesDiscovered
            | Self::NoSuitesMatchedFilters
            | Self::TrialTimedOut { .. }
            | Self::Message { .. } => None,
        }
    }
}

impl From<std::io::Error> for EvalError {
    fn from(value: std::io::Error) -> Self {
        Self::Io {
            message: value.to_string(),
        }
    }
}

impl From<serde_json::Error> for EvalError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde {
            message: value.to_string(),
        }
    }
}

fn format_duration_ms(duration_ms: u64) -> String {
    humantime::format_duration(std::time::Duration::from_millis(duration_ms)).to_string()
}
