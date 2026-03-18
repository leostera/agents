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

    /// Returns the serialized partial trial, if one was attached.
    pub fn partial_trial_json(&self) -> Option<&serde_json::Value> {
        match self {
            Self::MessageWithTrial { trial, .. } => Some(trial.as_ref()),
            Self::Io { .. } | Self::Serde { .. } | Self::Message { .. } => None,
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
