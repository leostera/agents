use std::sync::Arc;

use agents::{
    Agent, AgentEvent, AgentInput, AgentResult, AgentRunInput, AgentRunOutput, ContextManager,
    InputItem, LlmRunner, SessionAgent,
};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
macro_rules! echo_info {
    ($($arg:tt)*) => {
        worker::console_log!($($arg)*);
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! echo_info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*);
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! echo_debug {
    ($($arg:tt)*) => {
        worker::console_debug!($($arg)*);
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! echo_debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*);
    };
}

static DEFAULT_PROMPT: &str = "You are an echo agent. Reply as JSON matching the CloudEchoResponse schema with the exact same text from the user's message in the `text` field.";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloudEchoRequest {
    pub text: String,
}

impl From<CloudEchoRequest> for InputItem {
    fn from(value: CloudEchoRequest) -> Self {
        InputItem::user_text(value.text)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloudEchoResponse {
    pub text: String,
}

pub struct CloudEchoAgent {
    agent: SessionAgent<CloudEchoRequest, (), (), String>,
}

impl CloudEchoAgent {
    pub async fn new(runner: Arc<LlmRunner>) -> Result<Self> {
        echo_info!("constructing CloudEchoAgent");
        let agent = SessionAgent::builder()
            .with_llm_runner(runner)
            .with_context_manager(ContextManager::static_text(DEFAULT_PROMPT))
            .with_message_type::<CloudEchoRequest>()
            .with_response_type::<String>()
            .build()?;

        Ok(Self { agent })
    }
}

impl Agent for CloudEchoAgent {
    type Input = CloudEchoRequest;
    type ToolCall = ();
    type ToolResult = ();
    type Output = String;

    async fn send(&mut self, input: AgentInput<Self::Input>) -> AgentResult<()> {
        echo_debug!("CloudEchoAgent::send input={:?}", input);
        self.agent.send(input).await
    }

    async fn next(
        &mut self,
    ) -> AgentResult<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>> {
        let event = self.agent.next().await?;
        echo_debug!("CloudEchoAgent::next event={:?}", event);
        Ok(event)
    }

    async fn spawn(
        self,
    ) -> AgentResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        echo_info!("CloudEchoAgent::spawn");
        self.agent.spawn().await
    }
}
