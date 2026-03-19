//! Public crate for building typed agents.
//!
//! `agents` includes:
//! - provider-neutral LLM requests and providers
//! - the `Agent` trait and `SessionAgent`
//! - tool, context, and storage integration
//! - agent-side derive macros like `#[derive(Agent)]` and `#[derive(Tool)]`

pub use agents_proc_macros::{Agent, Tool};

pub mod agent;
pub mod llm;

pub use agent::*;
pub use llm::*;
