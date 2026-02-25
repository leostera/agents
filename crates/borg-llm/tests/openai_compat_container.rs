use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::testing::llm_container::LlmContainer;
use borg_llm::{LlmRequest, Provider, ProviderMessage, UserBlock};

#[tokio::test]
async fn openai_provider_chat_against_vllm_container() {
    let llm = LlmContainer::start_vllm().await.unwrap();
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

    let response = provider.chat(&request).await.unwrap();
    assert!(
        !response.content.is_empty(),
        "assistant message should contain at least one content block"
    );
}
