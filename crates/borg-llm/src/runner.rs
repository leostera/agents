use std::any::{Any, TypeId};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::completion::{
    CompletionEvent, CompletionEventStream, CompletionRequest, CompletionResponse, InputItem,
    ModelSelector, OutputContent, OutputItem, RawCompletionEvent, RawCompletionRequest,
    RawCompletionResponse, RawInputContent, RawInputItem, RawOutputContent, RawOutputItem,
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
            input: req
                .input
                .into_iter()
                .map(|item| match item {
                    InputItem::Message { role, content } => RawInputItem::Message {
                        role,
                        content: content
                            .into_iter()
                            .map(|content| match content {
                                crate::completion::InputContent::Text { text } => {
                                    RawInputContent::Text { text }
                                }
                                crate::completion::InputContent::ImageUrl { url } => {
                                    RawInputContent::ImageUrl { url }
                                }
                            })
                            .collect(),
                    },
                    InputItem::ToolResult {
                        tool_use_id,
                        content,
                    } => RawInputItem::ToolResult {
                        tool_use_id,
                        content,
                    },
                })
                .collect(),
            temperature: req.temperature,
            top_p: req.top_p,
            top_k: req.top_k,
            token_limit: req.token_limit,
            response_mode: req.response_mode,
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

        match serde_json::from_str(&content) {
            Ok(value) => Ok(value),
            Err(original_error) => {
                if let Some(candidate) = extract_json_candidate(&content) {
                    serde_json::from_str(candidate)
                        .map_err(|_| Error::parse(content, original_error))
                } else {
                    Err(Error::parse(content, original_error))
                }
            }
        }
    }

    fn from_raw_response<C, R>(raw: RawCompletionResponse) -> LlmResult<CompletionResponse<C, R>>
    where
        C: TypedTool,
        R: DeserializeOwned + 'static,
    {
        let output = raw
            .output
            .into_iter()
            .map(|item| match item {
                RawOutputItem::Message { role, content } => {
                    let content = content
                        .into_iter()
                        .map(|content| match content {
                            RawOutputContent::Text { text } => {
                                if TypeId::of::<R>() == TypeId::of::<String>() {
                                    Ok(OutputContent::Text { text })
                                } else {
                                    match Self::decode_response_content::<R>(text.clone()) {
                                        Ok(value) => Ok(OutputContent::Structured { value }),
                                        Err(_) => Ok(OutputContent::Text { text }),
                                    }
                                }
                            }
                            RawOutputContent::Json { value } => {
                                let encoded = serde_json::to_string(&value).map_err(|error| {
                                    Error::Internal {
                                        message: error.to_string(),
                                    }
                                })?;
                                Ok(OutputContent::Structured {
                                    value: Self::decode_response_content::<R>(encoded)?,
                                })
                            }
                        })
                        .collect::<LlmResult<Vec<_>>>()?;

                    Ok(OutputItem::Message { role, content })
                }
                RawOutputItem::ToolCall { call } => {
                    let tool = C::decode_tool_call(&call.name, call.arguments)?;
                    Ok(OutputItem::ToolCall {
                        call: ToolCall { id: call.id, tool },
                    })
                }
                RawOutputItem::Reasoning { text } => Ok(OutputItem::Reasoning { text }),
            })
            .collect::<LlmResult<Vec<_>>>()?;

        Ok(CompletionResponse {
            provider: raw.provider,
            model: raw.model,
            output,
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

    pub async fn chat_stream<C, R>(
        &self,
        req: CompletionRequest<C, R>,
    ) -> LlmResult<CompletionEventStream<C, R>>
    where
        C: TypedTool + Send,
        R: DeserializeOwned + JsonSchema + Send + 'static,
    {
        let provider = self.find_provider(&req.model)?;
        let mut raw_stream = provider
            .chat_raw_stream(Self::into_raw_request(req))
            .await?;
        let (sender, receiver) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Some(event) = raw_stream.recv().await {
                let mapped = match event {
                    Ok(RawCompletionEvent::TextDelta { text }) => {
                        Ok(CompletionEvent::TextDelta { text })
                    }
                    Ok(RawCompletionEvent::ReasoningDelta { text }) => {
                        Ok(CompletionEvent::ReasoningDelta { text })
                    }
                    Ok(RawCompletionEvent::ToolCall { call }) => {
                        match C::decode_tool_call(&call.name, call.arguments) {
                            Ok(tool) => Ok(CompletionEvent::ToolCall {
                                call: crate::tools::ToolCall { id: call.id, tool },
                            }),
                            Err(error) => Err(error),
                        }
                    }
                    Ok(RawCompletionEvent::Done(raw)) => {
                        Self::from_raw_response(raw).map(CompletionEvent::Done)
                    }
                    Err(error) => Err(error),
                };

                if sender.send(mapped).await.is_err() {
                    break;
                }
            }
        });

        Ok(CompletionEventStream::new(receiver))
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

fn extract_json_candidate(content: &str) -> Option<&str> {
    let trimmed = content.trim();

    if let Some(fenced) = trimmed.strip_prefix("```json") {
        let fenced = fenced.trim();
        return fenced.strip_suffix("```").map(str::trim);
    }

    if let Some(fenced) = trimmed.strip_prefix("```") {
        let fenced = fenced.trim();
        return fenced.strip_suffix("```").map(str::trim);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::LlmRunner;
    use crate::capability::Capability;
    use crate::completion::{
        CompletionEvent, CompletionRequest, FinishReason, InputItem, ModelSelector, OutputContent,
        OutputItem, ProviderType, RawCompletionEvent, RawCompletionEventStream,
        RawCompletionRequest, RawCompletionResponse, RawOutputContent, RawOutputItem, ResponseMode,
        Role, Usage,
    };
    use crate::error::{Error, LlmResult};
    use crate::model::Model;
    use crate::provider::LlmProvider;
    use crate::tools::{RawToolCall, RawToolDefinition, TypedTool};
    use crate::transcription::{AudioTranscriptionRequest, AudioTranscriptionResponse};
    use async_trait::async_trait;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use tokio::sync::mpsc;

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

    struct StreamingTestProvider;

    #[async_trait]
    impl LlmProvider for StreamingTestProvider {
        fn provider_type(&self) -> ProviderType {
            ProviderType::Ollama
        }

        fn provider_name(&self) -> &'static str {
            "streaming-test"
        }

        fn capabilities(&self) -> &[Capability] {
            &[Capability::ChatCompletion]
        }

        async fn available_models(&self) -> LlmResult<Vec<Model>> {
            Ok(vec![Model::new("streaming-test-model")])
        }

        async fn chat_raw(&self, _req: RawCompletionRequest) -> LlmResult<RawCompletionResponse> {
            Err(Error::Internal {
                message: "chat_raw should not be used in this streaming test".to_string(),
            })
        }

        async fn chat_raw_stream(
            &self,
            _req: RawCompletionRequest,
        ) -> LlmResult<RawCompletionEventStream> {
            let (sender, receiver) = mpsc::channel(4);
            sender
                .send(Ok(RawCompletionEvent::TextDelta {
                    text: "{\"value\":\"typed".to_string(),
                }))
                .await
                .expect("receiver should be alive");
            sender
                .send(Ok(RawCompletionEvent::TextDelta {
                    text: "-ok\"}".to_string(),
                }))
                .await
                .expect("receiver should be alive");
            sender
                .send(Ok(RawCompletionEvent::ToolCall {
                    call: RawToolCall {
                        id: "call_1".to_string(),
                        name: "ping".to_string(),
                        arguments: serde_json::json!({ "value": "hello-tool" }),
                    },
                }))
                .await
                .expect("receiver should be alive");
            sender
                .send(Ok(RawCompletionEvent::Done(RawCompletionResponse {
                    provider: ProviderType::Ollama,
                    model: "streaming-test-model".to_string(),
                    output: vec![
                        RawOutputItem::Message {
                            role: Role::Assistant,
                            content: vec![RawOutputContent::Text {
                                text: r#"{"value":"typed-ok"}"#.to_string(),
                            }],
                        },
                        RawOutputItem::ToolCall {
                            call: RawToolCall {
                                id: "call_1".to_string(),
                                name: "ping".to_string(),
                                arguments: serde_json::json!({ "value": "hello-tool" }),
                            },
                        },
                    ],
                    usage: Usage {
                        prompt_tokens: 1,
                        completion_tokens: 2,
                        total_tokens: 3,
                    },
                    finish_reason: FinishReason::ToolCalls,
                })))
                .await
                .expect("receiver should be alive");
            Ok(RawCompletionEventStream::new(receiver))
        }

        async fn transcribe(
            &self,
            _req: AudioTranscriptionRequest,
        ) -> LlmResult<AudioTranscriptionResponse> {
            Err(Error::NoMatchingProvider {
                reason: "transcription not supported in test provider".to_string(),
            })
        }
    }

    #[test]
    fn from_raw_response_decodes_typed_content_and_tool_calls() {
        let raw = RawCompletionResponse {
            provider: ProviderType::Ollama,
            model: "test-model".to_string(),
            output: vec![
                RawOutputItem::Message {
                    role: Role::Assistant,
                    content: vec![RawOutputContent::Text {
                        text: r#"{"value":"typed-ok"}"#.to_string(),
                    }],
                },
                RawOutputItem::ToolCall {
                    call: RawToolCall {
                        id: "call_1".to_string(),
                        name: "ping".to_string(),
                        arguments: serde_json::json!({ "value": "hello-tool" }),
                    },
                },
            ],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::ToolCalls,
        };

        let response = LlmRunner::from_raw_response::<TestTools, TestResponse>(raw)
            .expect("raw response should decode into typed response");

        let typed_response = response.output.iter().find_map(|item| match item {
            OutputItem::Message { content, .. } => content.iter().find_map(|content| match content {
                OutputContent::Structured { value } => Some(value),
                OutputContent::Text { .. } => None,
            }),
            OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
        });

        assert_eq!(
            typed_response,
            Some(&TestResponse {
                value: "typed-ok".to_string(),
            })
        );
        let tool_calls = response
            .output
            .iter()
            .filter_map(|item| match item {
                OutputItem::ToolCall { call } => Some(call),
                OutputItem::Message { .. } | OutputItem::Reasoning { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0].tool,
            TestTools::Ping {
                value: "hello-tool".to_string(),
            }
        );
    }

    #[test]
    fn decode_response_content_accepts_fenced_json() {
        let decoded = LlmRunner::decode_response_content::<TestResponse>(
            "```json\n{\n  \"value\": \"typed-ok\"\n}\n```".to_string(),
        )
        .expect("fenced json should decode");

        assert_eq!(
            decoded,
            TestResponse {
                value: "typed-ok".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn chat_stream_maps_raw_events_into_typed_events() {
        let runner = LlmRunner::builder()
            .add_provider(StreamingTestProvider)
            .build();

        let mut stream = runner
            .chat_stream::<TestTools, TestResponse>(
                CompletionRequest::new(
                    vec![InputItem::user_text("hello")],
                    ModelSelector::from_model("streaming-test-model"),
                )
                .with_response_mode(ResponseMode::Stream),
            )
            .await
            .expect("stream should start");

        let first = stream
            .recv()
            .await
            .expect("first event should exist")
            .expect("first event should decode");
        assert!(matches!(
            first,
            CompletionEvent::TextDelta { ref text } if text == "{\"value\":\"typed"
        ));

        let second = stream
            .recv()
            .await
            .expect("second event should exist")
            .expect("second event should decode");
        assert!(matches!(
            second,
            CompletionEvent::TextDelta { ref text } if text == "-ok\"}"
        ));

        let third = stream
            .recv()
            .await
            .expect("third event should exist")
            .expect("third event should decode");
        assert!(matches!(
            third,
            CompletionEvent::ToolCall { ref call }
                if call.tool == TestTools::Ping { value: "hello-tool".to_string() }
        ));

        let fourth = stream
            .recv()
            .await
            .expect("done event should exist")
            .expect("done event should decode");
        match fourth {
            CompletionEvent::Done(response) => {
                let typed_response = response.output.iter().find_map(|item| match item {
                    OutputItem::Message { content, .. } => {
                        content.iter().find_map(|content| match content {
                            OutputContent::Structured { value } => Some(value),
                            OutputContent::Text { .. } => None,
                        })
                    }
                    OutputItem::ToolCall { .. } | OutputItem::Reasoning { .. } => None,
                });
                assert_eq!(
                    typed_response,
                    Some(&TestResponse {
                        value: "typed-ok".to_string(),
                    })
                );
                let final_tool_calls = response
                    .output
                    .iter()
                    .filter_map(|item| match item {
                        OutputItem::ToolCall { call } => Some(call),
                        OutputItem::Message { .. } | OutputItem::Reasoning { .. } => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(final_tool_calls.len(), 1);
            }
            other => panic!("expected done event, got {other:?}"),
        }

        assert!(stream.recv().await.is_none());
    }
}
