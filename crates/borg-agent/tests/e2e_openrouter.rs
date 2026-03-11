use borg_agent::{Agent, AgentInput, TurnOutcome};
use borg_llm::completion::InputItem;
use borg_llm::error::LlmResult;
use borg_llm::testing::{optional_test_env, runner_with_openrouter_model};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EchoResponse {
    value: String,
}

fn openrouter_model() -> String {
    optional_test_env("BORG_TEST_OPENROUTER_MODEL")
        .expect("BORG_TEST_OPENROUTER_MODEL must be set for OpenRouter e2e tests")
}

#[tokio::test]
async fn openrouter_agent_send_completes_text_turn_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    let report = agent
        .send::<_, String>(AgentInput::Message(InputItem::user_text(
            "Reply with a short plain-text acknowledgment. Do not return JSON.",
        )))
        .await
        .expect("turn");

    match report.outcome {
        TurnOutcome::Completed { reply } => {
            assert!(
                !reply.trim().is_empty(),
                "expected non-empty OpenRouter reply, got {:?}",
                reply
            );
        }
        TurnOutcome::Cancelled => panic!("unexpected cancellation"),
    }

    Ok(())
}

#[tokio::test]
async fn openrouter_agent_send_decodes_typed_response_long() -> LlmResult<()> {
    let model = openrouter_model();
    let runner = runner_with_openrouter_model(&model)?;

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .build()
        .expect("agent");

    let report = agent
        .send::<_, EchoResponse>(AgentInput::Message(InputItem::user_text(
            "Return valid JSON with a non-empty string field named value.",
        )))
        .await
        .expect("turn");

    match report.outcome {
        TurnOutcome::Completed { reply } => {
            assert!(
                !reply.value.trim().is_empty(),
                "expected non-empty typed OpenRouter reply, got {:?}",
                reply
            );
        }
        TurnOutcome::Cancelled => panic!("unexpected cancellation"),
    }

    Ok(())
}
