pub mod ollama_container;

use std::collections::HashSet;
use std::sync::{Arc, Once};

use tokio::sync::{Mutex, OnceCell};

use crate::error::LlmResult;
use crate::provider::ollama::{Ollama, OllamaConfig};
use crate::runner::LlmRunner;
use ollama_container::LlmContainer;

static TRACING: Once = Once::new();
static OLLAMA_CONTEXT: OnceCell<Arc<TestContext>> = OnceCell::const_new();

pub fn init_tracing() {
    TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_test_writer()
            .try_init();
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
            OllamaConfig::new()
                .with_base_url(self.base_url.clone())
                .with_default_model(model.to_string()),
        ))
    }
}
