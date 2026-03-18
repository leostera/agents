use std::sync::{Arc, Mutex};

use crate::llm::LlmRunner;
use crate::llm::completion::{InputContent, InputItem, Role};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::error::AgentResult;

/// Strategy hint for how a context chunk should be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextStrategy {
    Pinnable,
    Compactable,
}

/// Role attached to a context message chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextRole {
    System,
    User,
    Assistant,
}

/// One item in an agent context window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContextChunk {
    Message {
        strategy: ContextStrategy,
        role: ContextRole,
        content: String,
    },
    ToolCall {
        strategy: ContextStrategy,
        id: String,
        name: String,
        args: Value,
    },
    ToolResult {
        strategy: ContextStrategy,
        id: String,
        result: Value,
    },
}

impl ContextChunk {
    pub fn system_text(strategy: ContextStrategy, content: impl Into<String>) -> Self {
        Self::Message {
            strategy,
            role: ContextRole::System,
            content: content.into(),
        }
    }

    pub fn user_text(strategy: ContextStrategy, content: impl Into<String>) -> Self {
        Self::Message {
            strategy,
            role: ContextRole::User,
            content: content.into(),
        }
    }

    pub fn assistant_text(strategy: ContextStrategy, content: impl Into<String>) -> Self {
        Self::Message {
            strategy,
            role: ContextRole::Assistant,
            content: content.into(),
        }
    }

    pub fn from_input_item(
        strategy: ContextStrategy,
        item: InputItem,
    ) -> Option<AgentResult<Self>> {
        match item {
            InputItem::Message { role, content } => {
                let text = flatten_input_content(content);
                let role = match role {
                    Role::System => ContextRole::System,
                    Role::User => ContextRole::User,
                    Role::Assistant => ContextRole::Assistant,
                };
                Some(Ok(Self::Message {
                    strategy,
                    role,
                    content: text,
                }))
            }
            InputItem::ToolCall { call } => Some(Ok(Self::ToolCall {
                strategy,
                id: call.id,
                name: call.name,
                args: call.arguments,
            })),
            InputItem::ToolResult {
                tool_use_id,
                content,
            } => Some(match serde_json::from_str::<Value>(&content) {
                Ok(result) => Ok(Self::ToolResult {
                    strategy,
                    id: tool_use_id,
                    result,
                }),
                Err(_) => Ok(Self::ToolResult {
                    strategy,
                    id: tool_use_id,
                    result: Value::String(content),
                }),
            }),
        }
    }

    pub fn to_input_item(&self) -> Option<AgentResult<InputItem>> {
        match self {
            ContextChunk::Message { role, content, .. } => Some(Ok(match role {
                ContextRole::System => InputItem::system_text(content.clone()),
                ContextRole::User => InputItem::user_text(content.clone()),
                ContextRole::Assistant => InputItem::assistant_text(content.clone()),
            })),
            ContextChunk::ToolCall { id, name, args, .. } => Some(Ok(InputItem::tool_call(
                id.clone(),
                name.clone(),
                args.clone(),
            ))),
            ContextChunk::ToolResult { id, result, .. } => Some(
                serde_json::to_string(result)
                    .map(|content| InputItem::tool_result(id.clone(), content))
                    .map_err(|error| crate::agent::error::AgentError::Internal {
                        message: error.to_string(),
                    }),
            ),
        }
    }
}

/// Materialized context window ready to be lowered into model input items.
#[derive(Debug, Clone, Default)]
pub struct ContextWindow {
    pub chunks: Vec<ContextChunk>,
}

impl ContextWindow {
    pub fn new(chunks: Vec<ContextChunk>) -> Self {
        Self { chunks }
    }

    pub fn to_input_items(&self) -> AgentResult<Vec<InputItem>> {
        self.chunks
            .iter()
            .filter_map(|chunk| chunk.to_input_item())
            .collect()
    }
}

/// Source of additional context chunks for an agent.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    async fn provide(&self) -> AgentResult<Vec<ContextChunk>>;
}

/// Builder for [`ContextManager`].
pub struct ContextManagerBuilder {
    providers: Vec<Arc<dyn ContextProvider>>,
}

impl ContextManagerBuilder {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add_provider<P>(mut self, provider: P) -> Self
    where
        P: ContextProvider + 'static,
    {
        self.providers.push(Arc::new(provider));
        self
    }

    pub fn build(self) -> ContextManager {
        ContextManager {
            providers: self.providers,
            history: Mutex::new(Vec::new()),
            llm: Mutex::new(None),
        }
    }
}

impl Default for ContextManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Composes static providers and conversation history into a context window.
pub struct ContextManager {
    providers: Vec<Arc<dyn ContextProvider>>,
    history: Mutex<Vec<ContextChunk>>,
    llm: Mutex<Option<Arc<LlmRunner>>>,
}

impl ContextManager {
    pub fn builder() -> ContextManagerBuilder {
        ContextManagerBuilder::new()
    }

    pub fn static_text(text: impl Into<String>) -> Self {
        Self::builder()
            .add_provider(StaticContextProvider::system_text(text))
            .build()
    }

    pub fn new() -> Self {
        Self::builder().build()
    }

    pub fn with_provider_arc(mut self, provider: Arc<dyn ContextProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn attach_llm_runner(&self, llm: Arc<LlmRunner>) {
        *self.llm.lock().expect("context llm") = Some(llm);
    }

    pub async fn push(&self, chunk: ContextChunk) -> AgentResult<()> {
        self.history.lock().expect("context history").push(chunk);
        Ok(())
    }

    pub async fn window(&self) -> AgentResult<ContextWindow> {
        let mut chunks = Vec::new();
        for provider in &self.providers {
            chunks.extend(provider.provide().await?);
        }
        chunks.extend(self.history.lock().expect("context history").clone());
        Ok(ContextWindow::new(chunks))
    }

    pub async fn history(&self) -> AgentResult<Vec<ContextChunk>> {
        Ok(self.history.lock().expect("context history").clone())
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple context provider backed by a fixed list of chunks.
pub struct StaticContextProvider {
    chunks: Vec<ContextChunk>,
}

impl StaticContextProvider {
    pub fn new(chunks: Vec<ContextChunk>) -> Self {
        Self { chunks }
    }

    pub fn system_text(text: impl Into<String>) -> Self {
        Self::new(vec![ContextChunk::system_text(
            ContextStrategy::Pinnable,
            text,
        )])
    }
}

#[async_trait]
impl ContextProvider for StaticContextProvider {
    async fn provide(&self) -> AgentResult<Vec<ContextChunk>> {
        Ok(self.chunks.clone())
    }
}

fn flatten_input_content(content: Vec<InputContent>) -> String {
    content
        .into_iter()
        .filter_map(|part| match part {
            InputContent::Text { text } => Some(text),
            InputContent::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::error::AgentError;

    struct FailingProvider;

    #[async_trait]
    impl ContextProvider for FailingProvider {
        async fn provide(&self) -> AgentResult<Vec<ContextChunk>> {
            Err(AgentError::Internal {
                message: "provider failed".to_string(),
            })
        }
    }

    #[test]
    fn from_input_item_maps_message_roles_and_flattens_text_parts() {
        let item = InputItem::Message {
            role: Role::Assistant,
            content: vec![
                InputContent::Text {
                    text: "hello".to_string(),
                },
                InputContent::ImageUrl {
                    url: "https://example.com/cat.png".to_string(),
                },
                InputContent::Text {
                    text: "world".to_string(),
                },
            ],
        };

        let chunk = ContextChunk::from_input_item(ContextStrategy::Compactable, item)
            .expect("chunk")
            .expect("valid chunk");

        assert_eq!(
            chunk,
            ContextChunk::assistant_text(ContextStrategy::Compactable, "hello\nworld")
        );
    }

    #[test]
    fn from_input_item_parses_json_tool_results() {
        let chunk = ContextChunk::from_input_item(
            ContextStrategy::Compactable,
            InputItem::tool_result("call_1", r#"{"status":"ok"}"#),
        )
        .expect("chunk")
        .expect("valid chunk");

        assert_eq!(
            chunk,
            ContextChunk::ToolResult {
                strategy: ContextStrategy::Compactable,
                id: "call_1".to_string(),
                result: serde_json::json!({ "status": "ok" }),
            }
        );
    }

    #[test]
    fn from_input_item_falls_back_to_string_for_non_json_tool_results() {
        let chunk = ContextChunk::from_input_item(
            ContextStrategy::Compactable,
            InputItem::tool_result("call_1", "plain text error"),
        )
        .expect("chunk")
        .expect("valid chunk");

        assert_eq!(
            chunk,
            ContextChunk::ToolResult {
                strategy: ContextStrategy::Compactable,
                id: "call_1".to_string(),
                result: Value::String("plain text error".to_string()),
            }
        );
    }

    #[test]
    fn tool_result_chunk_round_trips_back_to_input_item() {
        let item = ContextChunk::ToolResult {
            strategy: ContextStrategy::Compactable,
            id: "call_1".to_string(),
            result: serde_json::json!({ "status": "ok" }),
        }
        .to_input_item()
        .expect("tool result lowers")
        .expect("valid item");

        assert!(matches!(
            item,
            InputItem::ToolResult { tool_use_id, content }
                if tool_use_id == "call_1" && content == r#"{"status":"ok"}"#
        ));
    }

    #[tokio::test]
    async fn static_provider_chunks_precede_history_in_window() {
        let manager = ContextManager::builder()
            .add_provider(StaticContextProvider::system_text("system prompt"))
            .build();

        manager
            .push(ContextChunk::user_text(
                ContextStrategy::Compactable,
                "hello from user",
            ))
            .await
            .expect("push");

        let window = manager.window().await.expect("window");
        assert_eq!(
            window.chunks,
            vec![
                ContextChunk::system_text(ContextStrategy::Pinnable, "system prompt"),
                ContextChunk::user_text(ContextStrategy::Compactable, "hello from user"),
            ]
        );
    }

    #[test]
    fn context_window_lowers_messages_tool_calls_and_tool_results() {
        let window = ContextWindow::new(vec![
            ContextChunk::system_text(ContextStrategy::Pinnable, "system"),
            ContextChunk::ToolCall {
                strategy: ContextStrategy::Compactable,
                id: "call_1".to_string(),
                name: "ping".to_string(),
                args: serde_json::json!({ "value": "hello" }),
            },
            ContextChunk::ToolResult {
                strategy: ContextStrategy::Compactable,
                id: "call_1".to_string(),
                result: serde_json::json!({ "status": "ok" }),
            },
        ]);

        let items = window.to_input_items().expect("input items");
        assert_eq!(items.len(), 3);
        assert!(matches!(
            &items[0],
            InputItem::Message {
                role: Role::System,
                ..
            }
        ));
        assert!(matches!(
            &items[1],
            InputItem::ToolCall { call }
                if call.id == "call_1"
                    && call.name == "ping"
                    && call.arguments == serde_json::json!({ "value": "hello" })
        ));
        assert!(matches!(
            &items[2],
            InputItem::ToolResult { tool_use_id, .. } if tool_use_id == "call_1"
        ));
    }

    #[tokio::test]
    async fn multiple_providers_preserve_builder_order_before_history() {
        let manager = ContextManager::builder()
            .add_provider(StaticContextProvider::new(vec![ContextChunk::system_text(
                ContextStrategy::Pinnable,
                "system one",
            )]))
            .add_provider(StaticContextProvider::new(vec![ContextChunk::system_text(
                ContextStrategy::Pinnable,
                "system two",
            )]))
            .build();

        manager
            .push(ContextChunk::user_text(
                ContextStrategy::Compactable,
                "hello from user",
            ))
            .await
            .expect("push");

        let window = manager.window().await.expect("window");
        assert_eq!(
            window.chunks,
            vec![
                ContextChunk::system_text(ContextStrategy::Pinnable, "system one"),
                ContextChunk::system_text(ContextStrategy::Pinnable, "system two"),
                ContextChunk::user_text(ContextStrategy::Compactable, "hello from user"),
            ]
        );
    }

    #[tokio::test]
    async fn push_preserves_history_order_and_window_is_non_destructive() {
        let manager = ContextManager::new();
        let first = ContextChunk::user_text(ContextStrategy::Compactable, "first");
        let second = ContextChunk::assistant_text(ContextStrategy::Compactable, "second");

        manager.push(first.clone()).await.expect("push first");
        manager.push(second.clone()).await.expect("push second");

        let history = manager.history().await.expect("history");
        assert_eq!(history, vec![first.clone(), second.clone()]);

        let window = manager.window().await.expect("window");
        assert_eq!(window.chunks, vec![first.clone(), second.clone()]);

        let history_again = manager.history().await.expect("history again");
        assert_eq!(history_again, vec![first, second]);
    }

    #[tokio::test]
    async fn static_text_builds_a_pinnable_system_message() {
        let manager = ContextManager::static_text("hello system");
        let window = manager.window().await.expect("window");

        assert_eq!(
            window.chunks,
            vec![ContextChunk::system_text(
                ContextStrategy::Pinnable,
                "hello system",
            )]
        );
    }

    #[tokio::test]
    async fn history_returns_only_session_history_not_provider_chunks() {
        let manager = ContextManager::builder()
            .add_provider(StaticContextProvider::system_text("system prompt"))
            .build();

        manager
            .push(ContextChunk::user_text(
                ContextStrategy::Compactable,
                "hello from user",
            ))
            .await
            .expect("push");

        let history = manager.history().await.expect("history");
        assert_eq!(
            history,
            vec![ContextChunk::user_text(
                ContextStrategy::Compactable,
                "hello from user",
            )]
        );
    }

    #[tokio::test]
    async fn failing_provider_errors_window() {
        let manager = ContextManager::builder()
            .add_provider(FailingProvider)
            .build();

        let error = manager.window().await.expect_err("provider should fail");
        assert!(matches!(error, AgentError::Internal { message } if message == "provider failed"));
    }

    #[tokio::test]
    async fn tool_calls_are_preserved_in_history_and_lowered_into_window() {
        let manager = ContextManager::new();

        manager
            .push(ContextChunk::ToolCall {
                strategy: ContextStrategy::Compactable,
                id: "call_1".to_string(),
                name: "ping".to_string(),
                args: serde_json::json!({ "value": "hello" }),
            })
            .await
            .expect("push");

        let history = manager.history().await.expect("history");
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0], ContextChunk::ToolCall { .. }));

        let input_items = manager
            .window()
            .await
            .expect("window")
            .to_input_items()
            .expect("items");
        assert_eq!(input_items.len(), 1);
        assert!(matches!(
            &input_items[0],
            InputItem::ToolCall { call }
                if call.id == "call_1"
                    && call.name == "ping"
                    && call.arguments == serde_json::json!({ "value": "hello" })
        ));
    }
}
