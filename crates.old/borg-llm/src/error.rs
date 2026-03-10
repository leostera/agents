use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("{0}")]
    Message(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("provider `{provider}` {capability} request failed with status {status}")]
    ProviderHttp {
        provider: &'static str,
        capability: &'static str,
        status: u16,
    },
    #[error("provider `{provider}` {capability} response error: {reason}")]
    ProviderResponse {
        provider: &'static str,
        capability: &'static str,
        reason: String,
    },
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl LlmError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, LlmError>;
