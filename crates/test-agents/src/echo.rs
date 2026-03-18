use std::sync::Arc;

use agents::prelude::*;
use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static DEFAULT_PROMPT: &str = "You are an echo agent. Always call the echo_text tool exactly once with the user's full text. Then reply as JSON matching the EchoRes schema with the same text in the `text` field.";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoRequest {
    pub pepo: String,
}

impl From<EchoRequest> for InputItem {
    fn from(value: EchoRequest) -> Self {
        InputItem::user_text(value.pepo)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoResponseFormat {
    pub boogyboo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Tool)]
pub enum EchoToolCall {
    #[agent_tool(
        name = "echo_text",
        description = "Return the exact input text unchanged."
    )]
    Echo { fartson: String },
}

#[derive(Clone)]
struct EchoToolRunner;

#[async_trait]
impl ToolRunner<EchoToolCall, EchoToolResponse> for EchoToolRunner {
    async fn run(
        &self,
        call: ToolCallEnvelope<EchoToolCall>,
    ) -> AgentResult<ToolResultEnvelope<EchoToolResponse>> {
        let result = match call.call {
            EchoToolCall::Echo { fartson } => ToolExecutionResult::Ok {
                data: EchoToolResponse { nanana: fartson },
            },
        };

        Ok(ToolResultEnvelope {
            call_id: call.call_id,
            result,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoToolResponse {
    nanana: String,
}

#[derive(Agent)]
pub struct EchoAgent {
    agent: SessionAgent<EchoRequest, EchoToolCall, EchoToolResponse, EchoResponseFormat>,
}

impl EchoAgent {
    pub async fn new(runner: Arc<LlmRunner>) -> Result<Self> {
        let agent = SessionAgent::builder()
            .with_llm_runner(runner)
            .with_tool_runner(EchoToolRunner)
            .with_context_manager(ContextManager::static_text(DEFAULT_PROMPT))
            .with_message_type::<EchoRequest>()
            .with_response_type::<EchoResponseFormat>()
            .build()?;

        Ok(Self { agent })
    }
}
