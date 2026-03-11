use borg_agent::{Agent, AgentInput, TurnOutcome};
use borg_llm::completion::InputItem;
use borg_llm::error::LlmResult;
use borg_llm::testing::{TestContext, TestProvider};
use serial_test::serial;

const OLLAMA_TEXT_MODEL: &str = "qwen2.5:7b";

#[tokio::test]
#[serial]
async fn ollama_agent_send_completes_text_turn_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    let report = agent
        .send(AgentInput::Message(InputItem::user_text(
            "Reply with a short plain-text acknowledgment. Do not return JSON.",
        )))
        .await
        .expect("turn");

    match report.outcome {
        TurnOutcome::Completed { reply } => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty Ollama reply, got {:?}",
                reply
            );
        }
        TurnOutcome::Cancelled => panic!("unexpected cancellation"),
    }

    Ok(())
}
