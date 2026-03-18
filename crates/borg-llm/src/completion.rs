use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{Error, LlmResult};
use crate::response::{RawResponseFormat, TypedResponse};
use crate::tools::{RawToolCall, RawToolDefinition, ToolCall, TypedToolSet};

/// LLM provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    OpenRouter,
    LmStudio,
    Ollama,
    Apple,
}

impl ProviderType {
    pub fn name(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::OpenRouter => "openrouter",
            ProviderType::LmStudio => "lm_studio",
            ProviderType::Ollama => "ollama",
            ProviderType::Apple => "apple",
        }
    }
}

/// Strategy for selecting a provider or exact model name.
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

/// Message role used in completion input and output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// One typed input item sent to a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InputItem {
    Message {
        role: Role,
        content: Vec<InputContent>,
    },
    ToolCall {
        call: RawToolCall,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl InputItem {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::Message {
            role: Role::User,
            content: vec![InputContent::text(text)],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::Message {
            role: Role::Assistant,
            content: vec![InputContent::text(text)],
        }
    }

    pub fn system_text(text: impl Into<String>) -> Self {
        Self::Message {
            role: Role::System,
            content: vec![InputContent::text(text)],
        }
    }

    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self::ToolCall {
            call: RawToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            },
        }
    }

    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
        }
    }
}

impl From<String> for InputItem {
    fn from(value: String) -> Self {
        Self::user_text(value)
    }
}

impl From<&str> for InputItem {
    fn from(value: &str) -> Self {
        Self::user_text(value)
    }
}

/// One content item inside a message input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InputContent {
    Text { text: String },
    ImageUrl { url: String },
}

impl InputContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn image_url(url: impl Into<String>) -> Self {
        Self::ImageUrl { url: url.into() }
    }
}

impl From<String> for InputContent {
    fn from(value: String) -> Self {
        Self::text(value)
    }
}

impl From<&str> for InputContent {
    fn from(value: &str) -> Self {
        Self::text(value)
    }
}

/// Reason a provider ended generation.
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

/// Typed completion request sent through [`crate::LlmRunner`].
#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct CompletionRequest<ToolType, ResponseType> {
    #[builder(default = "ModelSelector::Any")]
    pub model: ModelSelector,
    pub input: Vec<InputItem>,
    #[builder(default = "Temperature::ProviderDefault")]
    pub temperature: Temperature,
    #[builder(default = "TopP::ProviderDefault")]
    pub top_p: TopP,
    #[builder(default = "TopK::ProviderDefault")]
    pub top_k: TopK,
    #[builder(default = "TokenLimit::ProviderDefault")]
    pub token_limit: TokenLimit,
    #[builder(default = "ResponseMode::Buffered")]
    pub response_mode: ResponseMode,
    #[builder(default)]
    pub tools: Option<TypedToolSet<ToolType>>,
    #[builder(default = "ToolChoice::ProviderDefault")]
    pub tool_choice: ToolChoice,
    #[builder(default)]
    pub response_format: Option<TypedResponse<ResponseType>>,
}

impl<ToolType, ResponseType> CompletionRequest<ToolType, ResponseType> {
    pub fn new(input: Vec<InputItem>, model: ModelSelector) -> Self {
        Self {
            model,
            input,
            temperature: Temperature::ProviderDefault,
            top_p: TopP::ProviderDefault,
            top_k: TopK::ProviderDefault,
            token_limit: TokenLimit::ProviderDefault,
            response_mode: ResponseMode::Buffered,
            tools: None,
            tool_choice: ToolChoice::ProviderDefault,
            response_format: None,
        }
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Temperature::Value(temperature);
        self
    }

    pub fn with_token_limit(mut self, token_limit: TokenLimit) -> Self {
        self.token_limit = token_limit;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.token_limit = TokenLimit::Max(max_tokens);
        self
    }

    pub fn with_top_p(mut self, top_p: Probability) -> Self {
        self.top_p = TopP::Value(top_p);
        self
    }

    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = TopK::Value(top_k);
        self
    }

    pub fn with_response_mode(mut self, response_mode: ResponseMode) -> Self {
        self.response_mode = response_mode;
        self
    }

    pub fn with_tools(mut self, tools: TypedToolSet<ToolType>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }

    pub fn with_typed_response(mut self, response_format: TypedResponse<ResponseType>) -> Self {
        self.response_format = Some(response_format);
        self
    }
}

/// Final typed completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResponse<ToolType = (), ResponseType = String> {
    pub provider: ProviderType,
    pub model: String,
    pub output: Vec<OutputItem<ToolType, ResponseType>>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

#[cfg(test)]
mod tests {
    use super::{InputContent, InputItem, Role};

    #[test]
    fn input_item_from_string_defaults_to_user_text() {
        let item = InputItem::from("hello");

        match item {
            InputItem::Message { role, content } => {
                assert_eq!(role, Role::User);
                assert_eq!(content.len(), 1);
                assert!(
                    matches!(content.first(), Some(InputContent::Text { text }) if text == "hello")
                );
            }
            other => panic!("expected user text message, got {other:?}"),
        }
    }

    #[test]
    fn input_content_from_string_defaults_to_text() {
        let content = InputContent::from("hello");
        assert!(matches!(content, InputContent::Text { text } if text == "hello"));
    }
}

/// One typed output item emitted by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum OutputItem<ToolType = (), ResponseType = String> {
    Message {
        role: Role,
        content: Vec<OutputContent<ResponseType>>,
    },
    ToolCall {
        call: ToolCall<ToolType>,
    },
    Reasoning {
        text: String,
    },
}

/// Content carried by a typed output message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum OutputContent<ResponseType = String> {
    Text { text: String },
    Structured { value: ResponseType },
}

/// Whether a response should be buffered or streamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResponseMode {
    Buffered,
    Stream,
}

impl ResponseMode {
    pub fn is_streaming(self) -> bool {
        matches!(self, Self::Stream)
    }
}

/// Probability value constrained to the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Probability(f32);

impl Probability {
    pub fn new(value: f32) -> LlmResult<Self> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidRequest {
                reason: format!("probability must be between 0.0 and 1.0, got {value}"),
            })
        }
    }

    pub fn value(self) -> f32 {
        self.0
    }
}

/// Temperature configuration for generation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Temperature {
    ProviderDefault,
    Value(f32),
}

impl Temperature {
    pub fn as_option(self) -> Option<f32> {
        match self {
            Self::ProviderDefault => None,
            Self::Value(value) => Some(value),
        }
    }
}

/// Token limit configuration for generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenLimit {
    ProviderDefault,
    Max(u32),
}

impl TokenLimit {
    pub fn as_option(self) -> Option<u32> {
        match self {
            Self::ProviderDefault => None,
            Self::Max(value) => Some(value),
        }
    }
}

/// Top-p sampling configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopP {
    ProviderDefault,
    Value(Probability),
}

impl TopP {
    pub fn as_option(self) -> Option<f32> {
        match self {
            Self::ProviderDefault => None,
            Self::Value(value) => Some(value.value()),
        }
    }
}

/// Top-k sampling configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopK {
    ProviderDefault,
    Value(u32),
}

impl TopK {
    pub fn as_option_i32(self) -> Option<i32> {
        match self {
            Self::ProviderDefault => None,
            Self::Value(value) => i32::try_from(value).ok(),
        }
    }
}

/// Tool selection mode for a completion request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolChoice {
    ProviderDefault,
    Auto,
    Required,
    Specific { name: String },
    None,
}

/// One streamed completion event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionEvent<ToolType, ResponseType> {
    TextDelta { text: String },
    ReasoningDelta { text: String },
    ToolCall { call: ToolCall<ToolType> },
    Done(CompletionResponse<ToolType, ResponseType>),
}

/// Stream of typed completion events.
pub struct CompletionEventStream<ToolType, ResponseType> {
    receiver: mpsc::Receiver<crate::error::LlmResult<CompletionEvent<ToolType, ResponseType>>>,
}

impl<ToolType, ResponseType> CompletionEventStream<ToolType, ResponseType> {
    pub fn new(
        receiver: mpsc::Receiver<crate::error::LlmResult<CompletionEvent<ToolType, ResponseType>>>,
    ) -> Self {
        Self { receiver }
    }

    pub async fn recv(
        &mut self,
    ) -> Option<crate::error::LlmResult<CompletionEvent<ToolType, ResponseType>>> {
        self.receiver.recv().await
    }
}

/// Untyped completion request sent directly to a provider implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCompletionRequest {
    pub model: ModelSelector,
    pub input: Vec<RawInputItem>,
    pub temperature: Temperature,
    pub top_p: TopP,
    pub top_k: TopK,
    pub token_limit: TokenLimit,
    pub response_mode: ResponseMode,
    pub tools: Option<Vec<RawToolDefinition>>,
    pub tool_choice: ToolChoice,
    pub response_format: Option<RawResponseFormat>,
}

/// Untyped completion response returned directly by a provider implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCompletionResponse {
    pub provider: ProviderType,
    pub model: String,
    pub output: Vec<RawOutputItem>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

/// Untyped input item used by provider implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RawInputItem {
    Message {
        role: Role,
        content: Vec<RawInputContent>,
    },
    ToolCall {
        call: RawToolCall,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Untyped content item used by provider implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RawInputContent {
    Text { text: String },
    ImageUrl { url: String },
}

/// Untyped output item used by provider implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RawOutputItem {
    Message {
        role: Role,
        content: Vec<RawOutputContent>,
    },
    ToolCall {
        call: RawToolCall,
    },
    Reasoning {
        text: String,
    },
}

/// Untyped output content used by provider implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RawOutputContent {
    Text { text: String },
    Json { value: serde_json::Value },
}

/// One streamed raw completion event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RawCompletionEvent {
    TextDelta { text: String },
    ReasoningDelta { text: String },
    ToolCall { call: RawToolCall },
    Done(RawCompletionResponse),
}

/// Stream of raw completion events.
pub struct RawCompletionEventStream {
    receiver: mpsc::Receiver<crate::error::LlmResult<RawCompletionEvent>>,
}

impl RawCompletionEventStream {
    pub fn new(receiver: mpsc::Receiver<crate::error::LlmResult<RawCompletionEvent>>) -> Self {
        Self { receiver }
    }

    pub async fn recv(&mut self) -> Option<crate::error::LlmResult<RawCompletionEvent>> {
        self.receiver.recv().await
    }
}

/// Token accounting attached to a provider response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
