use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum OpenAIConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone)]
pub enum AnthropicConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone)]
pub enum OpenRouterConfigError {
    #[error("API key is required")]
    MissingApiKey,
}

#[derive(Error, Debug, Clone)]
pub enum LmStudioConfigError {
    #[error("Base URL is required")]
    MissingBaseUrl,
}

#[derive(Error, Debug, Clone)]
pub enum OllamaConfigError {
    #[error("Base URL is required")]
    MissingBaseUrl,
}

#[derive(Debug)]
pub struct ParseError {
    pub value: String,
    pub error: serde_json::Error,
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

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    OpenAIConfig(OpenAIConfigError),

    #[error(transparent)]
    AnthropicConfig(AnthropicConfigError),

    #[error(transparent)]
    OpenRouterConfig(OpenRouterConfigError),

    #[error(transparent)]
    LmStudioConfig(LmStudioConfigError),

    #[error(transparent)]
    OllamaConfig(OllamaConfigError),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("HTTP error: {source}")]
    Http {
        #[from]
        source: reqwest::Error,
    },

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
}

impl Error {
    pub fn parse(value: impl Into<String>, error: serde_json::Error) -> Self {
        Error::Parse {
            source: ParseError {
                value: value.into(),
                error,
            },
        }
    }
}

pub type LlmResult<V> = std::result::Result<V, Error>;
