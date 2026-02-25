mod agent;
mod context;
mod llm_adapter;
mod message;
mod session;
mod tools;

pub use agent::{
    Agent, DEFAULT_AGENT_ID, DEFAULT_MAX_TURNS, DEFAULT_MODEL, DEFAULT_SYSTEM_PROMPT,
};
pub use context::{ContextManager, ContextWindow, PassThroughContextManager};
pub use llm_adapter::{to_provider_messages, tool_result_to_text};
pub use message::{
    Message, SessionEndStatus, SessionEventPayload, SessionOutput, SessionResult, ToolCallRecord,
};
pub use session::Session;
pub use tools::{
    AgentTools, CapabilitySummary, ToolRequest, ToolResponse, ToolResultData, ToolRunner, ToolSpec,
    call_tool, default_tool_specs, to_provider_tool_specs,
};

#[cfg(test)]
mod tests;
