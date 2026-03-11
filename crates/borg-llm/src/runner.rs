use std::any::{Any, TypeId};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::completion::{
    CompletionRequest, CompletionResponse, Message, ModelSelector, RawCompletionRequest,
    RawCompletionResponse,
};
use crate::error::{Error, LlmResult};
use crate::provider::LlmProvider;
use crate::tools::{ToolCall, TypedTool};
use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};

pub struct LlmRunner {
    providers: Vec<Arc<dyn LlmProvider>>,
}

pub struct LlmRunnerBuilder {
    providers: Vec<Arc<dyn LlmProvider>>,
}

impl LlmRunnerBuilder {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add_provider<P: LlmProvider + 'static>(mut self, provider: P) -> Self {
        self.providers.push(Arc::new(provider));
        self
    }

    pub fn build(self) -> LlmRunner {
        LlmRunner {
            providers: self.providers,
        }
    }
}

impl Default for LlmRunnerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmRunner {
    pub fn builder() -> LlmRunnerBuilder {
        LlmRunnerBuilder::new()
    }

    fn find_provider(&self, model: &ModelSelector) -> LlmResult<Arc<dyn LlmProvider>> {
        match model {
            ModelSelector::Any => {
                self.providers
                    .first()
                    .cloned()
                    .ok_or(Error::NoMatchingProvider {
                        reason: "No providers available".to_string(),
                    })
            }
            ModelSelector::Provider(provider_type) => self
                .providers
                .iter()
                .find(|p| p.provider_type() == *provider_type)
                .cloned()
                .ok_or(Error::NoMatchingProvider {
                    reason: format!("No provider found for type {:?}", provider_type),
                }),
            ModelSelector::Specific { provider, .. } => {
                if let Some(provider_type) = provider {
                    self.providers
                        .iter()
                        .find(|p| p.provider_type() == *provider_type)
                        .cloned()
                        .ok_or(Error::NoMatchingProvider {
                            reason: format!("No provider found for type {:?}", provider_type),
                        })
                } else {
                    self.providers
                        .first()
                        .cloned()
                        .ok_or(Error::NoMatchingProvider {
                            reason: "No providers available".to_string(),
                        })
                }
            }
        }
    }

    fn into_raw_request<C, R>(req: CompletionRequest<C, R>) -> RawCompletionRequest
    where
        C: TypedTool,
        R: JsonSchema,
    {
        RawCompletionRequest {
            model: req.model,
            messages: req.messages,
            temperature: req.temperature,
            top_p: req.top_p,
            top_k: req.top_k,
            max_tokens: req.max_tokens,
            stream: req.stream,
            tools: req.tools.map(|tools| tools.to_tool_definitions()),
            tool_choice: req.tool_choice,
            response_format: req
                .response_format
                .map(|response_format| response_format.to_raw_response_format()),
        }
    }

    fn decode_response_content<R>(content: String) -> LlmResult<R>
    where
        R: DeserializeOwned + 'static,
    {
        if TypeId::of::<R>() == TypeId::of::<String>() {
            let boxed: Box<dyn Any> = Box::new(content);
            return boxed
                .downcast::<R>()
                .map(|value| *value)
                .map_err(|_| Error::Internal {
                    message: "failed to downcast string response".to_string(),
                });
        }

        serde_json::from_str(&content).map_err(|e| Error::parse(content, e))
    }

    fn from_raw_response<C, R>(raw: RawCompletionResponse) -> LlmResult<CompletionResponse<C, R>>
    where
        C: TypedTool,
        R: DeserializeOwned + 'static,
    {
        let tool_calls = raw
            .tool_calls
            .into_iter()
            .map(|call| {
                let tool = C::decode_tool_call(&call.name, call.arguments)?;
                Ok(ToolCall { id: call.id, tool })
            })
            .collect::<LlmResult<Vec<_>>>()?;

        Ok(CompletionResponse {
            provider: raw.provider,
            model: raw.model,
            message: Message {
                role: raw.message.role,
                content: Self::decode_response_content(raw.message.content)?,
            },
            tool_calls,
            usage: raw.usage,
            finish_reason: raw.finish_reason,
        })
    }

    pub async fn chat<C, R>(
        &self,
        req: CompletionRequest<C, R>,
    ) -> LlmResult<CompletionResponse<C, R>>
    where
        C: TypedTool,
        R: DeserializeOwned + JsonSchema + 'static,
    {
        let provider = self.find_provider(&req.model)?;
        let raw = provider.chat_raw(Self::into_raw_request(req)).await?;
        Self::from_raw_response(raw)
    }

    pub async fn transcribe(
        &self,
        req: AudioTranscriptionRequest,
    ) -> LlmResult<AudioTranscriptionResponse> {
        self.providers
            .iter()
            .find(|p| p.capabilities().iter().any(|c| c.supports_transcription()))
            .cloned()
            .ok_or(Error::NoMatchingProvider {
                reason: "No provider supports audio transcription".to_string(),
            })?
            .transcribe(req)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::LlmRunner;
    use crate::completion::{FinishReason, Message, ProviderType, RawCompletionResponse, Usage};
    use crate::error::{Error, LlmResult};
    use crate::tools::{RawToolCall, RawToolDefinition, TypedTool};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    struct TestResponse {
        value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    enum TestTools {
        Ping { value: String },
    }

    impl TypedTool for TestTools {
        fn tool_definitions() -> Vec<RawToolDefinition> {
            vec![RawToolDefinition::function(
                "ping",
                Some("Ping tool"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"]
                }),
            )]
        }

        fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
            match name {
                "ping" => Ok(Self::Ping {
                    value: arguments
                        .get("value")
                        .and_then(|value| value.as_str())
                        .ok_or(Error::InvalidResponse {
                            reason: "missing ping.value".to_string(),
                        })?
                        .to_string(),
                }),
                other => Err(Error::InvalidResponse {
                    reason: format!("unexpected tool name: {other}"),
                }),
            }
        }
    }

    #[test]
    fn from_raw_response_decodes_typed_content_and_tool_calls() {
        let raw = RawCompletionResponse {
            provider: ProviderType::Ollama,
            model: "test-model".to_string(),
            message: Message::assistant(r#"{"value":"typed-ok"}"#),
            tool_calls: vec![RawToolCall {
                id: "call_1".to_string(),
                name: "ping".to_string(),
                arguments: serde_json::json!({ "value": "hello-tool" }),
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::ToolCalls,
        };

        let response = LlmRunner::from_raw_response::<TestTools, TestResponse>(raw)
            .expect("raw response should decode into typed response");

        assert_eq!(
            response.message.content,
            TestResponse {
                value: "typed-ok".to_string(),
            }
        );
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(
            response.tool_calls[0].tool,
            TestTools::Ping {
                value: "hello-tool".to_string(),
            }
        );
    }
}
