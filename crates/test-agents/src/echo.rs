use anyhow::Result;
use borg_agent::{AgentEvent, AgentInput, AgentRunInput, AgentRunOutput};
use borg_evals_core::{EvalAgent, EvalResult, async_trait};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Clone, Default)]
pub struct EchoHarness;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoReq(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoRes(pub String);

#[derive(Clone)]
pub struct EchoAgent;

impl EchoAgent {
    pub async fn new() -> Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl EvalAgent for EchoAgent {
    type Input = EchoReq;
    type ToolCall = ();
    type ToolResult = ();
    type Output = EchoRes;

    async fn run(
        self,
    ) -> EvalResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )> {
        let (input_tx, mut input_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);

        tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                match input {
                    AgentInput::Message(EchoReq(text)) | AgentInput::Steer(EchoReq(text)) => {
                        let _ = event_tx
                            .send(Ok(AgentEvent::Completed {
                                reply: EchoRes(text),
                            }))
                            .await;
                    }
                    AgentInput::Cancel => {
                        let _ = event_tx.send(Ok(AgentEvent::Cancelled)).await;
                        break;
                    }
                }
            }
        });

        Ok((input_tx, event_rx))
    }
}
