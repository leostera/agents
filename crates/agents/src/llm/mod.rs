//! Provider-neutral typed APIs for completions, tools, and transcription.
//!
//! `agents::llm` is the provider-neutral layer in the stack:
//!
//! - build a [`LlmRunner`] from one or more providers
//! - submit typed [`CompletionRequest`] values
//! - receive typed [`CompletionResponse`] values or streamed [`CompletionEvent`]s
//!
//! # Example
//!
//! ```rust,no_run
//! use agents::{CompletionRequest, InputItem, LlmRunner, ModelSelector};
//!
//! # async fn demo(runner: LlmRunner) -> anyhow::Result<()> {
//!
//! let response = runner
//!     .chat(CompletionRequest::<(), String>::new(
//!         vec![InputItem::user_text("hello")],
//!         ModelSelector::from_model("gpt-5.3-codex"),
//!     ))
//!     .await?;
//! # Ok(()) }
//! ```
pub mod capability;
pub mod completion;
pub mod error;
pub mod model;
pub mod provider;
pub mod response;
mod runner;
pub mod testing;
pub mod tools;
pub mod transcription;

pub use completion::*;
pub use runner::*;
