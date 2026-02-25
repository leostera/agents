use std::time::Duration;

use anyhow::Result;
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::time::sleep;

const OLLAMA_IMAGE_NAME: &str = "ollama/ollama";
const OLLAMA_IMAGE_TAG: &str = "latest";
const OLLAMA_PORT: u16 = 11434;
const DEFAULT_TEST_MODEL: &str = "qwen2.5:0.5b";
const DEFAULT_TEST_API_KEY: &str = "test-key";
const TAGS_PATH: &str = "/api/tags";
const PULL_PATH: &str = "/api/pull";
const MAX_READINESS_ATTEMPTS: usize = 300;
const READINESS_BACKOFF_MILLIS: u64 = 1000;
const MAX_PULL_ATTEMPTS: usize = 5400;
const PULL_BACKOFF_MILLIS: u64 = 1000;

pub struct LlmContainer {
    _container: ContainerAsync<GenericImage>,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

impl LlmContainer {
    pub async fn start_vllm() -> Result<Self> {
        Self::start_ollama_with_model(DEFAULT_TEST_MODEL).await
    }

    pub async fn start_ollama_with_model(model: impl Into<String>) -> Result<Self> {
        let model = model.into();
        let container = GenericImage::new(OLLAMA_IMAGE_NAME, OLLAMA_IMAGE_TAG)
            .with_exposed_port(OLLAMA_PORT.tcp())
            .start()
            .await?;

        let host_port = container.get_host_port_ipv4(OLLAMA_PORT.tcp()).await?;
        let base_url = format!("http://127.0.0.1:{host_port}");
        wait_until_ready(&base_url).await?;
        pull_model(&base_url, &model).await?;

        Ok(Self {
            _container: container,
            base_url,
            model,
            api_key: DEFAULT_TEST_API_KEY.to_string(),
        })
    }
}

async fn wait_until_ready(base_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}{TAGS_PATH}");
    for _ in 0..MAX_READINESS_ATTEMPTS {
        let response = client.get(&url).send().await;
        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(READINESS_BACKOFF_MILLIS)).await;
    }

    anyhow::bail!("ollama container never became ready at {}", url);
}

async fn pull_model(base_url: &str, model: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}{PULL_PATH}");
    for _ in 0..MAX_PULL_ATTEMPTS {
        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "model": model,
                "stream": false
            }))
            .send()
            .await;
        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(PULL_BACKOFF_MILLIS)).await;
    }

    anyhow::bail!("ollama model pull never completed for {} at {}", model, url);
}
