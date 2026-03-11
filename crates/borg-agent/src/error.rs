use borg_llm::error::Error as LlmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Invalid input: {reason}")]
    InvalidInput { reason: String },

    #[error("Invalid response: {reason}")]
    InvalidResponse { reason: String },

    #[error("Cancelled")]
    Cancelled,

    #[error("Internal error: {message}")]
    Internal { message: String },
}

pub type AgentResult<T> = Result<T, AgentError>;
