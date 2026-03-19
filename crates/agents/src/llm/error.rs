use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum OpenAIConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum AnthropicConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum OpenRouterConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum WorkersAIConfigError {
    #[error("API token is required")]
    MissingApiToken,
    #[error("Account ID is required")]
    MissingAccountId,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum LmStudioConfigError {
    #[error("Base URL is required")]
    MissingBaseUrl,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum OllamaConfigError {
    #[error("Base URL is required")]
    MissingBaseUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParseError {
    pub value: String,
    pub error: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to parse JSON: {}\n\
             raw value:\n{}",
            self.error, self.value
        )
    }
}

impl std::error::Error for ParseError {}

/// Error returned by provider setup, request execution, or response decoding.
#[derive(Error, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Error {
    #[error(transparent)]
    OpenAIConfig(OpenAIConfigError),

    #[error(transparent)]
    AnthropicConfig(AnthropicConfigError),

    #[error(transparent)]
    OpenRouterConfig(OpenRouterConfigError),

    #[error(transparent)]
    WorkersAIConfig(WorkersAIConfigError),

    #[error(transparent)]
    LmStudioConfig(LmStudioConfigError),

    #[error(transparent)]
    OllamaConfig(OllamaConfigError),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("HTTP error: {message}")]
    Http { message: String },

    #[error("Failed to parse response: {source}")]
    Parse {
        #[from]
        source: ParseError,
    },

    #[error("Provider error [{provider}] (status {status}): {message}")]
    Provider {
        provider: String,
        status: u16,
        message: String,
    },

    #[error("Rate limited by {provider}: retry after {retry_after}s")]
    RateLimited { provider: String, retry_after: u64 },

    #[error("Authentication error for {provider}: {message}")]
    Authentication { provider: String, message: String },

    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },

    #[error("Invalid response: {reason}")]
    InvalidResponse { reason: String },

    #[error("Resource not found: {resource_type}/{id}")]
    NotFound { resource_type: String, id: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("No provider found matching request: {reason}")]
    NoMatchingProvider { reason: String },

    #[error("All providers failed: {errors:?}")]
    AllProvidersFailed { errors: Vec<Error> },
}

impl Error {
    pub fn parse(value: impl Into<String>, error: serde_json::Error) -> Self {
        Error::Parse {
            source: ParseError {
                value: value.into(),
                error: error.to_string(),
            },
        }
    }

    pub fn from_eventsource(
        provider: impl Into<String>,
        error: reqwest_eventsource::Error,
    ) -> Self {
        let provider = provider.into();
        match error {
            reqwest_eventsource::Error::Transport(source) => Error::from(source),
            reqwest_eventsource::Error::InvalidStatusCode(status, _) => Error::Provider {
                provider,
                status: status.as_u16(),
                message: format!("invalid event stream status: {status}"),
            },
            reqwest_eventsource::Error::InvalidContentType(value, _) => Error::InvalidResponse {
                reason: format!("invalid event stream content type: {value:?}"),
            },
            reqwest_eventsource::Error::Utf8(error) => Error::InvalidResponse {
                reason: error.to_string(),
            },
            reqwest_eventsource::Error::Parser(error) => Error::InvalidResponse {
                reason: error.to_string(),
            },
            reqwest_eventsource::Error::InvalidLastEventId(value) => Error::InvalidRequest {
                reason: format!("invalid last event id: {value}"),
            },
            reqwest_eventsource::Error::StreamEnded => Error::InvalidResponse {
                reason: "event stream ended unexpectedly".to_string(),
            },
        }
    }

    pub fn from_eventsource_builder(
        provider: impl Into<String>,
        error: reqwest_eventsource::CannotCloneRequestError,
    ) -> Self {
        Error::InvalidRequest {
            reason: format!(
                "{provider} streaming request could not be cloned: {error}",
                provider = provider.into()
            ),
        }
    }

    pub fn provider_name(&self) -> Option<&str> {
        match self {
            Error::Provider { provider, .. } => Some(provider.as_str()),
            Error::RateLimited { provider, .. } => Some(provider.as_str()),
            Error::Authentication { provider, .. } => Some(provider.as_str()),
            _ => None,
        }
    }

    pub fn provider_status(&self) -> Option<u16> {
        match self {
            Error::Provider { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Http { .. } => true,
            Error::RateLimited { .. } => true,
            Error::Provider { status, .. } => *status == 429 || (500..=599).contains(status),
            _ => false,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Http {
            message: value.to_string(),
        }
    }
}

/// Result type used by `agents::llm`.
pub type LlmResult<V> = std::result::Result<V, Error>;

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn provider_errors_expose_provider_metadata() {
        let error = Error::Provider {
            provider: "openai".to_string(),
            status: 503,
            message: "unavailable".to_string(),
        };

        assert_eq!(error.provider_name(), Some("openai"));
        assert_eq!(error.provider_status(), Some(503));
        assert!(error.is_retryable());
    }

    #[test]
    fn invalid_requests_are_not_retryable() {
        let error = Error::InvalidRequest {
            reason: "bad schema".to_string(),
        };

        assert_eq!(error.provider_name(), None);
        assert_eq!(error.provider_status(), None);
        assert!(!error.is_retryable());
    }
}
