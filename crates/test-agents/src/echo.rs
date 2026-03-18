use anyhow::Result;
use borg_agent::*;
use borg_llm::completion::InputItem;
use borg_llm::runner::LlmRunner;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static DEFAULT_PROMPT: &'static str = "You are an echo agent. Always call the echo_text tool exactly once with the user's full text. Then reply as JSON matching the EchoRes schema with the same text in the `text` field.";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoRequest(pub String);

impl From<EchoRequest> for InputItem {
    fn from(value: EchoRequest) -> Self {
        InputItem::user_text(value.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoResponseFormat {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, borg_macros::Tool)]
pub enum EchoToolCall {
    #[agent_tool(
        name = "echo_text",
        description = "Return the exact input text unchanged."
    )]
    Echo { text: String },
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
            EchoToolCall::Echo { text } => ToolExecutionResult::Ok {
                data: EchoToolResponse(text),
            },
        };

        Ok(ToolResultEnvelope {
            call_id: call.call_id,
            result,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoToolResponse(String);

#[derive(Agent)]
pub struct EchoAgent {
    agent: SessionAgent<EchoRequest, EchoToolCall, EchoToolResponse, EchoResponseFormat>,
}

impl EchoAgent {
    pub async fn new(runner: LlmRunner) -> Result<Self> {
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
