#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use agents::{
    Agent, AgentResult, ContextManager, InputItem, LlmRunner, SessionAgent, Tool, ToolCallEnvelope,
    ToolExecutionResult, ToolResultEnvelope, ToolRunner,
};
use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static DEFAULT_PROMPT: &str = "You are an echo agent. Always call the echo_text tool exactly once with the user's full text. Then reply as JSON matching the CloudEchoResponse schema with the same text in the `text` field.";

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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Tool)]
pub enum CloudEchoToolCall {
    #[agent_tool(
        name = "echo_text",
        description = "Return the exact input text unchanged."
    )]
    Echo { text: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloudEchoToolResponse {
    text: String,
}

#[derive(Clone)]
struct CloudEchoToolRunner;

#[async_trait]
impl ToolRunner<CloudEchoToolCall, CloudEchoToolResponse> for CloudEchoToolRunner {
    async fn run(
        &self,
        call: ToolCallEnvelope<CloudEchoToolCall>,
    ) -> AgentResult<ToolResultEnvelope<CloudEchoToolResponse>> {
        let result = match call.call {
            CloudEchoToolCall::Echo { text } => ToolExecutionResult::Ok {
                data: CloudEchoToolResponse { text },
            },
        };

        Ok(ToolResultEnvelope {
            call_id: call.call_id,
            result,
        })
    }
}

#[derive(Agent)]
pub struct CloudEchoAgent {
    #[agent]
    agent:
        SessionAgent<CloudEchoRequest, CloudEchoToolCall, CloudEchoToolResponse, CloudEchoResponse>,
}

impl CloudEchoAgent {
    pub async fn new(runner: Arc<LlmRunner>) -> Result<Self> {
        let agent = SessionAgent::builder()
            .with_llm_runner(runner)
            .with_tool_runner(CloudEchoToolRunner)
            .with_context_manager(ContextManager::static_text(DEFAULT_PROMPT))
            .with_message_type::<CloudEchoRequest>()
            .with_response_type::<CloudEchoResponse>()
            .build()?;

        Ok(Self { agent })
    }
}
