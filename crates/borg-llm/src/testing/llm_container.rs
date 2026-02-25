use std::time::Duration;

use anyhow::Result;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::time::sleep;

const VLLM_IMAGE_NAME: &str = "vllm/vllm-openai";
const VLLM_IMAGE_TAG: &str = "latest";
const VLLM_PORT: u16 = 8000;
const DEFAULT_TEST_MODEL: &str = "Qwen/Qwen2.5-0.5B-Instruct";
const DEFAULT_TEST_API_KEY: &str = "test-key";
const MODELS_PATH: &str = "/v1/models";
const MAX_READINESS_ATTEMPTS: usize = 240;
const READINESS_BACKOFF_MILLIS: u64 = 500;

pub struct LlmContainer {
    _container: ContainerAsync<GenericImage>,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

impl LlmContainer {
    pub async fn start_vllm() -> Result<Self> {
        Self::start_vllm_with_model(DEFAULT_TEST_MODEL).await
    }

    pub async fn start_vllm_with_model(model: impl Into<String>) -> Result<Self> {
        let model = model.into();
        let container = GenericImage::new(VLLM_IMAGE_NAME, VLLM_IMAGE_TAG)
            .with_exposed_port(VLLM_PORT.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Uvicorn running on"))
            .with_cmd(vec!["--model", model.as_str()])
            .start()
            .await?;

        let host_port = container.get_host_port_ipv4(VLLM_PORT.tcp()).await?;
        let base_url = format!("http://127.0.0.1:{host_port}");
        wait_until_ready(&base_url).await?;

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
    let url = format!("{base_url}{MODELS_PATH}");
    for _ in 0..MAX_READINESS_ATTEMPTS {
        let response = client
            .get(&url)
            .bearer_auth(DEFAULT_TEST_API_KEY)
            .send()
            .await;
        if let Ok(response) = response {
            if response.status().is_success() {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(READINESS_BACKOFF_MILLIS)).await;
    }

    anyhow::bail!("vllm container never became ready at {}", url);
}
