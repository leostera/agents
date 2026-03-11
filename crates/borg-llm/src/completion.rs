use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use crate::response::{RawResponseFormat, TypedResponse};
use crate::tools::{RawToolCall, RawToolChoice, RawToolDefinition, ToolCall, TypedToolSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    OpenRouter,
    LmStudio,
    Ollama,
}

impl ProviderType {
    pub fn name(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::OpenRouter => "openrouter",
            ProviderType::LmStudio => "lm_studio",
            ProviderType::Ollama => "ollama",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSelector {
    Any,
    Provider(ProviderType),
    Specific {
        provider: Option<ProviderType>,
        model: String,
    },
}

impl ModelSelector {
    pub fn any() -> Self {
        ModelSelector::Any
    }

    pub fn from_model(model: impl Into<String>) -> Self {
        ModelSelector::Specific {
            provider: None,
            model: model.into(),
        }
    }

    pub fn for_provider(provider: ProviderType) -> Self {
        ModelSelector::Provider(provider)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message<Content = String> {
    pub role: Role,
    pub content: Content,
}

impl Message<String> {
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Message {
            role: Role::Assistant,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Message {
            role: Role::Tool,
            content: content.into(),
        }
    }
}

impl<Content> Message<Content> {
    pub fn new(role: Role, content: Content) -> Self {
        Message { role, content }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Unknown(String),
}

impl From<Option<String>> for FinishReason {
    fn from(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            Some(other) => FinishReason::Unknown(other.to_string()),
            None => FinishReason::Unknown("null".to_string()),
        }
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct CompletionRequest<ToolType = (), ResponseType = String> {
    #[builder(default = "ModelSelector::Any")]
    pub model: ModelSelector,
    pub messages: Vec<Message<String>>,
    #[builder(default)]
    pub temperature: Option<f32>,
    #[builder(default)]
    pub top_p: Option<f32>,
    #[builder(default)]
    pub top_k: Option<i32>,
    #[builder(default)]
    pub max_tokens: Option<u32>,
    #[builder(default)]
    pub stream: Option<bool>,
    #[builder(default)]
    pub tools: Option<TypedToolSet<ToolType>>,
    #[builder(default)]
    pub tool_choice: Option<RawToolChoice>,
    #[builder(default)]
    pub response_format: Option<TypedResponse<ResponseType>>,
}

impl<ToolType, ResponseType> CompletionRequest<ToolType, ResponseType> {
    pub fn new(messages: Vec<Message<String>>, model: ModelSelector) -> Self {
        Self {
            model,
            messages,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            stream: None,
            tools: None,
            tool_choice: None,
            response_format: None,
        }
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_tools(mut self, tools: TypedToolSet<ToolType>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: RawToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    pub fn with_typed_response(mut self, response_format: TypedResponse<ResponseType>) -> Self {
        self.response_format = Some(response_format);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResponse<ToolType = (), ResponseType = String> {
    pub provider: ProviderType,
    pub model: String,
    pub message: Message<ResponseType>,
    pub tool_calls: Vec<ToolCall<ToolType>>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCompletionRequest {
    pub model: ModelSelector,
    pub messages: Vec<Message<String>>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub tools: Option<Vec<RawToolDefinition>>,
    pub tool_choice: Option<RawToolChoice>,
    pub response_format: Option<RawResponseFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCompletionResponse {
    pub provider: ProviderType,
    pub model: String,
    pub message: Message<String>,
    pub tool_calls: Vec<RawToolCall>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
