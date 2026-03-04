mod admin_tools;
mod agent;
mod context;
mod llm_adapter;
mod message;
mod session;
mod tools;

pub use admin_tools::{build_agent_admin_toolchain, default_agent_admin_tool_specs};
pub use agent::{Agent, DEFAULT_AGENT_ID, DEFAULT_MAX_TURNS, DEFAULT_MODEL, DEFAULT_SYSTEM_PROMPT};
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
