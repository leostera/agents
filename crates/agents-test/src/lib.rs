//! Test helpers for [`agents`].
//!
//! `agents-test` contains opt-in support for provider-backed integration tests,
//! including local Ollama container helpers and shared runner builders.

mod ollama_container;

use std::collections::HashSet;
use std::sync::{Arc, Once};

use agents::LlmRunner;
use agents::error::{Error as LlmError, LlmResult};
use agents::provider::anthropic::{Anthropic, AnthropicConfig};
use agents::provider::ollama::{Ollama, OllamaConfig};
use agents::provider::openai::{OpenAI, OpenAIConfig};
use agents::provider::openrouter::{OpenRouter, OpenRouterConfig};
use ollama_container::LlmContainer;
use tokio::sync::{Mutex, OnceCell};

static DOTENV: Once = Once::new();
static OLLAMA_CONTEXT: OnceCell<Arc<TestContext>> = OnceCell::const_new();

pub fn init_tracing() {
    // Intentionally a no-op. The caller owns logging/tracing configuration.
}

pub fn init_test_env() {
    DOTENV.call_once(|| {
        let _ = dotenvy::dotenv();
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestProvider {
    Ollama,
}

pub struct TestContext {
    provider: TestProvider,
    base_url: String,
    runtime: TestRuntime,
}

enum TestRuntime {
    Ollama(SharedOllamaServer),
}

struct SharedOllamaServer {
    container: LlmContainer,
    ensured_models: Mutex<HashSet<String>>,
}

impl TestContext {
    pub async fn shared(provider: TestProvider) -> LlmResult<Arc<Self>> {
        init_tracing();
        init_test_env();

        match provider {
            TestProvider::Ollama => OLLAMA_CONTEXT
                .get_or_try_init(|| async {
                    let container = LlmContainer::start_ollama().await?;
                    Ok(Arc::new(Self {
                        provider,
                        base_url: container.base_url.clone(),
                        runtime: TestRuntime::Ollama(SharedOllamaServer {
                            container,
                            ensured_models: Mutex::new(HashSet::new()),
                        }),
                    }))
                })
                .await
                .map(Arc::clone),
        }
    }

    pub fn provider(&self) -> TestProvider {
        self.provider
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn ensure_model(&self, model: &str) -> LlmResult<()> {
        match &self.runtime {
            TestRuntime::Ollama(server) => {
                let mut ensured = server.ensured_models.lock().await;
                if ensured.contains(model) {
                    return Ok(());
                }

                server.container.ensure_model(model).await?;
                ensured.insert(model.to_string());
                Ok(())
            }
        }
    }

    pub async fn runner_for_model(&self, model: &str) -> LlmResult<LlmRunner> {
        self.ensure_model(model).await?;
        Ok(match self.provider {
            TestProvider::Ollama => LlmRunner::builder()
                .add_provider(self.ollama_provider_for_model(model).await?)
                .build(),
        })
    }

    pub async fn ollama_provider_for_model(&self, model: &str) -> LlmResult<Ollama> {
        self.ensure_model(model).await?;
        Ok(Ollama::new(
            OllamaConfig::new(model.to_string()).with_base_url(self.base_url.clone()),
        ))
    }
}

pub fn required_test_env(name: &str) -> LlmResult<String> {
    init_test_env();
    std::env::var(name).map_err(|_| LlmError::Configuration(format!("missing test env var {name}")))
}

pub fn optional_test_env(name: &str) -> Option<String> {
    init_test_env();
    std::env::var(name).ok()
}

pub fn openai_provider_for_model(model: &str) -> LlmResult<OpenAI> {
    let api_key = required_test_env("BORG_TEST_OPENAI_API_KEY")?;
    let config = OpenAIConfig::new(api_key, model.to_string()).map_err(LlmError::OpenAIConfig)?;
    Ok(OpenAI::new(config))
}

pub fn anthropic_provider_for_model(model: &str) -> LlmResult<Anthropic> {
    let api_key = required_test_env("BORG_TEST_ANTHROPIC_API_KEY")?;
    let config =
        AnthropicConfig::new(api_key, model.to_string()).map_err(LlmError::AnthropicConfig)?;
    Ok(Anthropic::new(config))
}

pub fn openrouter_provider_for_model(model: &str) -> LlmResult<OpenRouter> {
    let api_key = required_test_env("BORG_TEST_OPENROUTER_API_KEY")?;
    let config =
        OpenRouterConfig::new(api_key, model.to_string()).map_err(LlmError::OpenRouterConfig)?;
    Ok(OpenRouter::new(config))
}

pub fn runner_with_openai_model(model: &str) -> LlmResult<LlmRunner> {
    Ok(LlmRunner::builder()
        .add_provider(openai_provider_for_model(model)?)
        .build())
}

pub fn runner_with_anthropic_model(model: &str) -> LlmResult<LlmRunner> {
    Ok(LlmRunner::builder()
        .add_provider(anthropic_provider_for_model(model)?)
        .build())
}

pub fn runner_with_openrouter_model(model: &str) -> LlmResult<LlmRunner> {
    Ok(LlmRunner::builder()
        .add_provider(openrouter_provider_for_model(model)?)
        .build())
}
