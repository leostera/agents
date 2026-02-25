use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod providers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndOfTurn,
    ToolCall,
    Aborted,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderBlock {
    Text(String),
    Thinking(String),
    ToolCall {
        id: String,
        name: String,
        arguments_json: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserBlock {
    Text(String),
    /// Media can be an image, a photo, a video, a voice message, etc
    Media {
        mime: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderMessage {
    System {
        text: String,
    },
    User {
        content: Vec<UserBlock>,
    },
    Assistant {
        content: Vec<ProviderBlock>,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: Vec<ProviderBlock>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDescriptor>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAssistantMessage {
    pub content: Vec<ProviderBlock>,
    pub stop_reason: StopReason,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage>;
}
