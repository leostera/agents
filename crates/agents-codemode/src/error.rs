use thiserror::Error;

/// Result type used throughout `agents-codemode`.
pub type CodeModeResult<T> = Result<T, CodeModeError>;

/// Errors produced by the codemode engine and its extension points.
#[derive(Debug, Error)]
pub enum CodeModeError {
    #[error("SearchCode requires a non-empty query")]
    EmptySearchQuery,

    #[error("native function not found: {name}")]
    NativeFunctionNotFound { name: String },

    #[error("failed to create codemode tokio runtime")]
    TokioRuntimeInit {
        #[source]
        source: std::io::Error,
    },

    #[error("codemode worker join error: {reason}")]
    WorkerJoin { reason: String },

    #[error("failed to install codemode globals: {reason}")]
    InstallGlobals { reason: String },

    #[error("failed to compile code: {reason}")]
    CompileCode { reason: String },

    #[error("RunCode.code must be an async zero-arg function expression")]
    InvalidClosureShape,

    #[error("RunCode.code did not resolve to a callable function")]
    ClosureNotCallable,

    #[error("failed to execute code: {reason}")]
    ExecuteCode { reason: String },

    #[error("codemode isolate panicked during execution")]
    IsolatePanicked,

    #[error("fetch worker panicked")]
    FetchWorkerPanicked,

    #[error("failed to serialize native function names")]
    NativeNamesSerialization {
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid fetch method `{method}`: {reason}")]
    InvalidFetchMethod { method: String, reason: String },

    #[error("fetch init.headers must be an object")]
    FetchHeadersNotObject,

    #[error("invalid header name `{key}`: {reason}")]
    InvalidHeaderName { key: String, reason: String },

    #[error("header `{key}` must be a string")]
    HeaderValueNotString { key: String },

    #[error("invalid value for header `{key}`: {reason}")]
    InvalidHeaderValue { key: String, reason: String },

    #[error(transparent)]
    Http {
        #[from]
        source: reqwest::Error,
    },
}
