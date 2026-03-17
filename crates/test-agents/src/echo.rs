use std::sync::Arc;

use anyhow::Result;
use borg_agent::{
    Agent, AgentResult, ContextManager, ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
    ToolRunner,
};
use borg_evals::{EvalAgent, EvalError, EvalResult, ExecutionTarget, async_trait};
use borg_llm::completion::InputItem;
use borg_llm::error::{Error, LlmResult};
use borg_llm::runner::LlmRunner;
use borg_llm::testing::{TestContext, TestProvider};
use borg_llm::tools::{RawToolDefinition, TypedTool};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct EchoHarness {
    ollama: Arc<TestContext>,
}

impl EchoHarness {
    pub async fn new() -> Result<Self> {
        let ollama = TestContext::shared(TestProvider::Ollama)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(Self { ollama })
    }

    pub async fn runner_for(&self, target: &ExecutionTarget) -> Result<LlmRunner> {
        self.ollama
            .runner_for_model(&target.model)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoReq(pub String);

impl From<EchoReq> for InputItem {
    fn from(value: EchoReq) -> Self {
        InputItem::user_text(value.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoRes {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoArgs {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum EchoTool {
    Echo(EchoArgs),
}

impl TypedTool for EchoTool {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        vec![RawToolDefinition::function(
            "echo_text",
            Some("Return the exact input text unchanged."),
            serde_json::to_value(schema_for!(EchoArgs)).expect("serialize EchoArgs schema"),
        )]
    }

    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
        match name {
            "echo_text" => Ok(Self::Echo(
                serde_json::from_value::<EchoArgs>(arguments)
                    .map_err(|error| Error::parse("tool arguments", error))?,
            )),
            other => Err(Error::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
}

#[derive(Clone)]
struct EchoToolRunner;

#[async_trait]
impl ToolRunner<EchoTool, String> for EchoToolRunner {
    async fn run(
        &self,
        call: ToolCallEnvelope<EchoTool>,
    ) -> AgentResult<ToolResultEnvelope<String>> {
        let result = match call.call {
            EchoTool::Echo(args) => ToolExecutionResult::Ok { data: args.text },
        };

        Ok(ToolResultEnvelope {
            call_id: call.call_id,
            result,
        })
    }
}

pub struct EchoAgent {
    agent: Agent<EchoReq, EchoTool, String, EchoRes>,
}

impl EchoAgent {
    pub async fn new(runner: LlmRunner) -> Result<Self> {
        let agent = Agent::builder()
            .with_llm_runner(runner)
            .with_tool_runner(EchoToolRunner)
            .with_context_manager(ContextManager::static_text(
                "You are an echo agent. Always call the echo_text tool exactly once with the user's full text. Then reply as JSON matching the EchoRes schema with the same text in the `text` field.",
            ))
            .with_message_type::<EchoReq>()
            .with_response_type::<EchoRes>()
            .build()?;

        Ok(Self { agent })
    }
}

#[async_trait]
impl EvalAgent for EchoAgent {
    type Input = EchoReq;
    type ToolCall = EchoTool;
    type ToolResult = String;
    type Output = EchoRes;

    async fn run(
        self,
    ) -> EvalResult<(
        borg_agent::AgentRunInput<Self::Input>,
        borg_agent::AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        self.agent
            .run()
            .await
            .map_err(|error| EvalError::message(error.to_string()))
    }
}
