use anyhow::Result;
use async_trait::async_trait;
use borg_exec::ExecEngine;

use crate::PortMessage;

#[derive(Clone)]
pub enum PortConfig {
    Http { exec: ExecEngine },
    Telegram { exec: ExecEngine, bot_token: String },
}

#[async_trait]
pub trait Port: Send + Sync {
    fn init(config: PortConfig) -> Result<Self>
    where
        Self: Sized;

    async fn handle_messages(&self, messages: Vec<PortMessage>) -> Vec<PortMessage>;
}
