mod actor_thread;
mod admin_tools;
mod agent;
mod context;
mod llm_adapter;
mod message;
mod tools;

pub use actor_thread::ActorThread;
pub use admin_tools::{build_actor_admin_toolchain, default_actor_admin_tool_specs};
pub use agent::{Agent, DEFAULT_MAX_TURNS};
pub use context::{
    ContextChunk, ContextChunkMode, ContextManager, ContextManagerBuilder, ContextManagerStrategy,
    ContextProvider, ContextWindow, StaticContextProvider,
};
pub use llm_adapter::to_provider_messages;
pub use message::{
    ActorEventPayload, ActorRunOutput, ActorRunResult, ActorRunStatus, Message, ToolCallRecord,
};
pub use tools::{
    BorgToolCall, BorgToolResult, BorgToolchain, Tool, ToolOutputEnvelope, ToolRequest,
    ToolResponse, ToolResultData, ToolSpec, Toolchain, ToolchainBuilder, to_provider_tool_specs,
};

#[cfg(test)]
mod tests;
