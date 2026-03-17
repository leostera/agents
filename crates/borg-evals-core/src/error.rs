use thiserror::Error;

use crate::trial::AgentTrial;

pub type EvalResult<T> = Result<T, EvalError>;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("eval failed: {message}")]
    Message { message: String },

    #[error("eval failed: {message}")]
    MessageWithTrial {
        message: String,
        trial: Box<AgentTrial>,
    },
}

impl EvalError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            message: message.into(),
        }
    }

    pub fn message_with_trial(message: impl Into<String>, trial: AgentTrial) -> Self {
        Self::MessageWithTrial {
            message: message.into(),
            trial: Box::new(trial),
        }
    }

    pub fn partial_trial(&self) -> Option<&AgentTrial> {
        match self {
            Self::MessageWithTrial { trial, .. } => Some(trial.as_ref()),
            Self::Io(_) | Self::Serde(_) | Self::Message { .. } => None,
        }
    }
}
