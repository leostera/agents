use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndOfTurn,
    ToolCall,
    Aborted,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReasoningEffort {
    type Err = ();

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "minimum" | "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::XHigh),
            _ => Err(()),
        }
    }
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
    pub reasoning_effort: Option<ReasoningEffort>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAssistantMessage {
    pub content: Vec<ProviderBlock>,
    pub stop_reason: StopReason,
    pub error_message: Option<String>,
    #[serde(default)]
    pub usage_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionRequest {
    pub audio: Vec<u8>,
    pub mime_type: String,
    pub model: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCodeAuthConfig {
    pub url: String,
    pub scope: Option<String>,
}

pub trait AuthProvider: Send + Sync {
    fn device_code_auth_config(&self) -> Option<DeviceCodeAuthConfig> {
        None
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    fn supports_chat_completion(&self) -> bool;

    fn supports_audio_transcription(&self) -> bool;

    async fn available_models(&self) -> Result<Vec<String>> {
        Err(crate::LlmError::configuration(format!(
            "provider `{}` does not support model discovery",
            self.provider_name()
        )))
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage>;
    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String>;
}

impl<T> AuthProvider for Arc<T>
where
    T: AuthProvider + ?Sized,
{
    fn device_code_auth_config(&self) -> Option<DeviceCodeAuthConfig> {
        self.as_ref().device_code_auth_config()
    }
}

#[async_trait]
impl<T> Provider for Arc<T>
where
    T: Provider + ?Sized,
{
    fn provider_name(&self) -> &'static str {
        self.as_ref().provider_name()
    }

    fn supports_chat_completion(&self) -> bool {
        self.as_ref().supports_chat_completion()
    }

    fn supports_audio_transcription(&self) -> bool {
        self.as_ref().supports_audio_transcription()
    }

    async fn chat(&self, req: &LlmRequest) -> Result<LlmAssistantMessage> {
        self.as_ref().chat(req).await
    }

    async fn available_models(&self) -> Result<Vec<String>> {
        self.as_ref().available_models().await
    }

    async fn transcribe(&self, req: &TranscriptionRequest) -> Result<String> {
        self.as_ref().transcribe(req).await
    }
}
