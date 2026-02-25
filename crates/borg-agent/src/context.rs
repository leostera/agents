use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Agent, Message, ToolSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[async_trait]
pub trait ContextManager: Send + Sync {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow>;
}

#[derive(Debug, Default)]
pub struct PassThroughContextManager;

#[async_trait]
impl ContextManager for PassThroughContextManager {
    async fn build_context(&self, agent: &Agent, messages: &[Message]) -> Result<ContextWindow> {
        Ok(ContextWindow {
            messages: messages.to_vec(),
            tools: agent.tools.clone(),
        })
    }
}
