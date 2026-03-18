use crate::llm::error::Error as LlmError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors returned while driving an agent.
#[derive(Debug, Error, Clone, Serialize, Deserialize, JsonSchema)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Invalid input: {reason}")]
    InvalidInput { reason: String },

    #[error("Invalid response: {reason}")]
    InvalidResponse { reason: String },

    #[error("Tool execution failed: {reason}")]
    ToolExecution { reason: String },

    #[error("Tool result encoding failed: {reason}")]
    ToolResultEncoding { reason: String },

    #[error("Cancelled")]
    Cancelled,

    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Result type used by agent operations.
pub type AgentResult<T> = Result<T, AgentError>;
