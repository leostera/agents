use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use borg_llm::completion::{InputContent, InputItem, Role};
use borg_llm::runner::LlmRunner;
use serde_json::Value;

use crate::error::AgentResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextStrategy {
    Pinnable,
    Compactable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
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
            ContextChunk::ToolCall { .. } => None,
            ContextChunk::ToolResult { id, result, .. } => Some(
                serde_json::to_string(result)
                    .map(|content| InputItem::tool_result(id.clone(), content))
                    .map_err(|error| crate::error::AgentError::Internal {
                        message: error.to_string(),
                    }),
            ),
        }
    }
}

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

#[async_trait]
pub trait ContextProvider: Send + Sync {
    async fn provide(&self) -> AgentResult<Vec<ContextChunk>>;
}

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

pub struct ContextManager {
    providers: Vec<Arc<dyn ContextProvider>>,
    history: Mutex<Vec<ContextChunk>>,
    llm: Mutex<Option<Arc<LlmRunner>>>,
}

impl ContextManager {
    pub fn builder() -> ContextManagerBuilder {
        ContextManagerBuilder::new()
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
    fn context_window_lowers_messages_and_tool_results_but_not_tool_calls() {
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
        assert_eq!(items.len(), 2);
        assert!(matches!(
            &items[0],
            InputItem::Message {
                role: Role::System,
                ..
            }
        ));
        assert!(matches!(
            &items[1],
            InputItem::ToolResult { tool_use_id, .. } if tool_use_id == "call_1"
        ));
    }
}
