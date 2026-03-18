//! Embeddable code execution and code search tools for custom agent tool runners.
//!
//! `agents-codemode` is intentionally small:
//! - [`CodeMode`] is the engine
//! - [`Request`] / [`Response`] are the typed boundary
//! - package, environment, and native function providers customize behavior
//!
//! The intended embedding path is:
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use agents_codemode::{CodeMode, CodeModeConfig, Request, Response};
//!
//! let codemode = Arc::new(
//!     CodeMode::builder()
//!         .with_config(CodeModeConfig::default().multithreaded(true))
//!         .build()?
//! );
//!
//! let response = codemode
//!     .execute(Request::SearchCode(agents_codemode::SearchCode {
//!         query: "fetch".to_string(),
//!         limit: Some(5),
//!     }))
//!     .await?;
//!
//! match response {
//!     Response::SearchCode(result) => {
//!         println!("{} matches", result.matches.len());
//!     }
//!     Response::RunCode(_) => unreachable!(),
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

mod config;
mod engine;
mod native;
mod providers;
mod request;

pub use config::CodeModeConfig;
pub use engine::{CodeMode, CodeModeBuilder};
pub use native::{NativeFunction, NativeFunctionRegistry};
pub use providers::{EnvironmentProvider, EnvironmentVariable, Package, PackageProvider};
pub use request::{
    PackageMatch, Request, Response, RunCode, RunCodeResult, SearchCode, SearchCodeResult,
};

#[cfg(test)]
mod tests;
