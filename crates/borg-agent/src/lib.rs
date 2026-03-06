mod admin_tools;
mod agent;
mod context;
mod llm_adapter;
mod message;
mod session;
mod tools;

pub use admin_tools::{build_actor_admin_toolchain, default_actor_admin_tool_specs};
pub use agent::{Agent, DEFAULT_MAX_TURNS};
pub use context::{
    ContextChunk, ContextChunkMode, ContextManager, ContextManagerBuilder, ContextManagerStrategy,
    ContextProvider, ContextWindow, StaticContextProvider,
};
pub use llm_adapter::{to_provider_messages, tool_result_to_text};
pub use message::{
    Message, SessionEndStatus, SessionEventPayload, SessionOutput, SessionResult, ToolCallRecord,
};
pub use session::Session;
pub use tools::{
    BorgToolCall, BorgToolResult, BorgToolchain, CapabilitySummary, Tool, ToolRequest,
    ToolResponse, ToolResultData, ToolSpec, Toolchain, ToolchainBuilder, to_provider_tool_specs,
};

#[cfg(test)]
mod tests;
