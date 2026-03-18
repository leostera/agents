//! Embeddable code execution and code search tools for custom agent tool runners.
//!
//! `agents-codemode` is intentionally small:
//! - [`CodeMode`] is the engine
//! - [`Request`] / [`Response`] are the typed boundary
//! - package, environment, and native function providers customize behavior
//!
//! The intended embedding path is:
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use agents_codemode::{CodeMode, CodeModeConfig, SearchCode};
//!
//! # tokio::runtime::Builder::new_current_thread()
//! #     .enable_all()
//! #     .build()?
//! #     .block_on(async {
//! let codemode = Arc::new(
//!     CodeMode::builder()
//!         .with_config(CodeModeConfig::default().multithreaded(true))
//!         .build()?
//! );
//!
//! let response = codemode
//!     .search_code(SearchCode {
//!         query: "fetch".to_string(),
//!         limit: Some(5),
//!     })
//!     .await?;
//!
//! println!("{} matches", response.matches.len());
//! # Ok::<(), anyhow::Error>(())
//! # })?;
//! # Ok::<(), anyhow::Error>(())
//! ```

mod config;
mod engine;
mod host;
mod module_loader;
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
