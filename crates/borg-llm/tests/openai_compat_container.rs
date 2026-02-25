use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::testing::llm_container::LlmContainer;
use borg_llm::{LlmRequest, Provider, ProviderMessage, UserBlock};
use std::sync::Once;
use tracing::{debug, info, trace};
use tracing_subscriber::EnvFilter;

fn init_test_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info,borg_llm=trace,borg_llm_test=trace")),
            )
            .with_test_writer()
            .try_init()
            .ok();
    });
}

#[tokio::test]
async fn openai_provider_chat_against_vllm_container() {
    init_test_tracing();
    info!(target: "borg_llm_it", "starting openai compatibility integration test");
    let llm = LlmContainer::start_ollama().await.unwrap();
    info!(
        target: "borg_llm_it",
        base_url = llm.base_url.as_str(),
        model = llm.model.as_str(),
        "ollama container started for openai compatibility test"
    );
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);
    let request = LlmRequest {
        model: llm.model.clone(),
        messages: vec![ProviderMessage::User {
            content: vec![UserBlock::Text(
                "Reply exactly with: BORG_TEST_OK".to_string(),
            )],
        }],
        tools: Vec::new(),
        temperature: Some(0.0),
        max_tokens: Some(32),
        api_key: Some(llm.api_key.clone()),
    };
    debug!(
        target: "borg_llm_it",
        message_count = request.messages.len(),
        max_tokens = request.max_tokens,
        "sending compatibility request to provider"
    );

    let response = provider.chat(&request).await.unwrap();
    trace!(target: "borg_llm_it", response = ?response, "received provider response");
    assert!(
        !response.content.is_empty(),
        "assistant message should contain at least one content block"
    );
    info!(target: "borg_llm_it", "openai compatibility integration test passed");
}
