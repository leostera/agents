use anyhow::Result;
use async_trait::async_trait;
use borg_agent::{
    Agent, AgentTools, Message, Session, SessionResult, ToolRequest, ToolResponse, ToolResultData,
    ToolRunner,
};
use borg_db::BorgDb;
use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::testing::llm_container::LlmContainer;
use std::path::PathBuf;
use uuid::Uuid;

struct NoopToolRunner;

#[async_trait]
impl ToolRunner for NoopToolRunner {
    async fn run(&self, _request: ToolRequest) -> Result<ToolResponse> {
        Ok(ToolResponse {
            content: ToolResultData::Text("noop".to_string()),
        })
    }
}

#[tokio::test]
async fn session_run_persists_events_with_real_llm() {
    let llm = LlmContainer::start_vllm().await.unwrap();
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);
    let db = make_test_db().await.unwrap();
    let agent_id = format!("borg:agent:{}", Uuid::now_v7());
    let session_id = format!("borg:session:{}", Uuid::now_v7());
    let agent = Agent::new(agent_id)
        .with_model(llm.model.clone())
        .with_system_prompt(
            "You are Borg. Reply briefly and include the word BORG_TEST_OK in your answer.",
        );
    let mut session = Session::new(session_id, agent.clone(), db).await.unwrap();
    session
        .add_message(Message::User {
            content: "Say hi".to_string(),
        })
        .await
        .unwrap();

    let tools = AgentTools {
        tool_runner: &NoopToolRunner,
    };
    let result = agent.run(&mut session, &provider, &tools).await;

    assert!(matches!(result, SessionResult::Completed(Ok(_))));
    let messages = session.read_messages(0, 512).await.unwrap();
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::SessionEvent { name, .. } if name == "agent_started"
    )));
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::SessionEvent { name, .. } if name == "agent_finished"
    )));
    assert!(
        messages.iter().any(
            |message| matches!(message, Message::Assistant { content } if !content.is_empty())
        )
    );
}

async fn make_test_db() -> Result<BorgDb> {
    let path = PathBuf::from(format!("/tmp/borg-agent-it-{}.db", Uuid::now_v7()));
    let borg_db = BorgDb::open_local(path.to_string_lossy().as_ref()).await?;
    borg_db.migrate().await?;
    Ok(borg_db)
}
