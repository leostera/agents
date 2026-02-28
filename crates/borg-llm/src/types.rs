use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::Result;

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
    fn provider_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn supports_chat_completion(&self) -> bool {
        true
    }

    fn supports_audio_transcription(&self) -> bool {
        true
    }

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
