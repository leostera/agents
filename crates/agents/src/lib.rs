//! Public facade for building agents.
//!
//! `agents` is the user-facing crate for:
//! - typed LLM requests and providers
//! - the `Agent` trait and `SessionAgent`
//! - tool, context, and storage integration
//! - agent-side derive macros like `#[derive(Agent)]` and `#[derive(Tool)]`

pub use agents_macros::{Agent, Tool};
pub use borg_agent as agent;
pub use borg_agent::*;
pub use borg_llm as llm;
pub use borg_llm::*;
