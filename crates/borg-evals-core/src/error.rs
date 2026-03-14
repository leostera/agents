use thiserror::Error;

pub type EvalResult<T> = Result<T, EvalError>;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("eval failed: {message}")]
    Message { message: String },
}

impl EvalError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            message: message.into(),
        }
    }
}

