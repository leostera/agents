//! Core agent traits and the default [`SessionAgent`] implementation.
//!
//! Most users do not implement an agent from scratch. The usual path is:
//!
//! 1. build a [`SessionAgent`] for your input/output types
//! 2. wrap it in your own struct
//! 3. derive [`Agent`](agents_proc_macros::Agent) to delegate the trait
//!
//! # String-in, string-out agent
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use agents::{LlmRunner, SessionAgent};
//!
//! async fn make_agent(
//!     llm: Arc<LlmRunner>,
//! ) -> anyhow::Result<SessionAgent<String, (), (), String>> {
//!     Ok(SessionAgent::builder().with_llm_runner(llm).build()?)
//! }
//! ```
//!
//! # Wrapped typed agent
//!
//! ```rust,no_run
//! use std::sync::Arc;
//!
//! use agents::{Agent as AgentTrait, InputItem, LlmRunner, SessionAgent};
//! use schemars::JsonSchema;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Clone, Serialize, Deserialize)]
//! struct EchoRequest {
//!     text: String,
//! }
//!
//! impl From<EchoRequest> for InputItem {
//!     fn from(value: EchoRequest) -> Self {
//!         InputItem::user_text(value.text)
//!     }
//! }
//!
//! #[derive(Clone, Serialize, Deserialize, JsonSchema)]
//! struct EchoResponse {
//!     text: String,
//! }
//!
//! #[derive(agents::Agent)]
//! struct EchoAgent {
//!     #[agent]
//!     inner: SessionAgent<EchoRequest, (), (), EchoResponse>,
//! }
//!
//! impl EchoAgent {
//!     async fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
//!         Ok(Self {
//!             inner: SessionAgent::builder()
//!                 .with_llm_runner(llm)
//!                 .with_message_type::<EchoRequest>()
//!                 .with_response_type::<EchoResponse>()
//!                 .build()?,
//!         })
//!     }
//! }
//! ```
mod context;
mod error;
mod runtime;
mod storage;
mod tools;

//pub use agents_proc_macros::Agent;
pub use context::{
    ContextChunk, ContextManager, ContextManagerBuilder, ContextProvider, ContextRole,
    ContextStrategy, ContextWindow, StaticContextProvider,
};
pub use error::{AgentError, AgentResult};
pub use runtime::{
    Agent, AgentBuilder, AgentEvent, AgentInput, AgentRunInput, AgentRunOutput, ExecutionProfile,
    PreparedRequest, SessionAgent,
};
pub use storage::{
    InMemoryStorageAdapter, NoopStorageAdapter, StorageAdapter, StorageEvent, StorageInput,
    StorageRecord,
};
pub use tools::{
    CallbackToolRunner, NoToolRunner, ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
    ToolRunner,
};
