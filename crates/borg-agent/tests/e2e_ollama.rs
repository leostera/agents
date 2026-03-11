use borg_agent::{Agent, AgentInput, ExecutionProfile, TurnOutcome};
use borg_llm::completion::Temperature;
use borg_llm::completion::{InputItem, ModelSelector, TokenLimit};
use borg_llm::error::LlmResult;
use borg_llm::testing::{TestContext, TestProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serial_test::serial;

const OLLAMA_TEXT_MODEL: &str = "qwen2.5:7b";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EchoResponse {
    value: String,
}

fn ollama_profile() -> ExecutionProfile {
    ExecutionProfile {
        model_selector: ModelSelector::from_model(OLLAMA_TEXT_MODEL),
        temperature: Temperature::Value(0.0),
        token_limit: TokenLimit::Max(64),
        ..ExecutionProfile::default()
    }
}

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
        .send_with_profile::<_, String>(
            AgentInput::Message(InputItem::user_text(
                "Reply with a short plain-text acknowledgment. Do not return JSON.",
            )),
            ollama_profile(),
        )
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

#[tokio::test]
#[serial]
async fn ollama_agent_send_twice_reuses_transcript_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    let first = agent
        .send_with_profile::<_, String>(
            AgentInput::Message(InputItem::user_text(
                "Remember this exact token for later: borg-agent-stage1. Reply with only OK.",
            )),
            ollama_profile(),
        )
        .await
        .expect("first turn");

    assert!(matches!(
        first.outcome,
        TurnOutcome::Completed { ref reply } if !reply.trim().is_empty()
    ));

    let second = agent
        .send_with_profile::<_, String>(
            AgentInput::Message(InputItem::user_text(
                "What exact token did I ask you to remember? Reply with only the token.",
            )),
            ollama_profile(),
        )
        .await
        .expect("second turn");

    match second.outcome {
        TurnOutcome::Completed { reply } => {
            assert!(
                reply.to_lowercase().contains("borg-agent-stage1"),
                "expected reply to reuse earlier transcript token, got {:?}",
                reply
            );
        }
        TurnOutcome::Cancelled => panic!("unexpected cancellation"),
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn ollama_agent_send_decodes_typed_response_long() -> LlmResult<()> {
    let ctx = TestContext::shared(TestProvider::Ollama).await?;
    let runner = ctx.runner_for_model(OLLAMA_TEXT_MODEL).await?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    let report = agent
        .send_with_profile::<_, EchoResponse>(
            AgentInput::Message(InputItem::user_text(
                "Return valid JSON with a non-empty string field named value.",
            )),
            ollama_profile(),
        )
        .await
        .expect("turn");

    match report.outcome {
        TurnOutcome::Completed { reply } => {
            assert!(
                !reply.value.trim().is_empty(),
                "expected non-empty typed Ollama reply, got {:?}",
                reply
            );
        }
        TurnOutcome::Cancelled => panic!("unexpected cancellation"),
    }

    Ok(())
}
