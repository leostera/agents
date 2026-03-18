use std::{path::PathBuf, time::Duration};

use testcontainers::core::{IntoContainerPort, Mount};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::time::sleep;
use tracing::{debug, error, info, trace};

use crate::llm::error::{Error, LlmResult};

const OLLAMA_IMAGE_NAME: &str = "ollama/ollama";
const OLLAMA_IMAGE_TAG: &str = "latest";
const OLLAMA_PORT: u16 = 11434;
const OLLAMA_MODELS_DIR_IN_CONTAINER: &str = "/root/.ollama";
const OLLAMA_MODELS_DIR_RELATIVE: &str = ".docker/volumes/ollama";
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
    pub api_key: String,
}

impl LlmContainer {
    pub async fn start_ollama() -> LlmResult<Self> {
        info!(
            target: "borg_llm_test",
            image = OLLAMA_IMAGE_NAME,
            tag = OLLAMA_IMAGE_TAG,
            "starting ollama test container"
        );
        let host_models_dir = host_ollama_models_dir()?;
        debug!(
            target: "borg_llm_test",
            host_models_dir = %host_models_dir.display(),
            "resolved host model cache directory"
        );
        let mount = Mount::bind_mount(
            host_models_dir.to_string_lossy().to_string(),
            OLLAMA_MODELS_DIR_IN_CONTAINER,
        );

        let container = GenericImage::new(OLLAMA_IMAGE_NAME, OLLAMA_IMAGE_TAG)
            .with_exposed_port(OLLAMA_PORT.tcp())
            .with_mount(mount)
            .start()
            .await
            .map_err(|err| Error::Internal {
                message: err.to_string(),
            })?;
        info!(target: "borg_llm_test", "ollama container started");

        let host_port = container
            .get_host_port_ipv4(OLLAMA_PORT.tcp())
            .await
            .map_err(|err| Error::Internal {
                message: err.to_string(),
            })?;
        let base_url = format!("http://127.0.0.1:{host_port}");
        info!(
            target: "borg_llm_test",
            base_url = base_url.as_str(),
            "resolved ollama container endpoint"
        );
        wait_until_ready(&base_url).await?;
        info!(target: "borg_llm_test", "ollama container ready");

        Ok(Self {
            _container: container,
            base_url,
            api_key: DEFAULT_TEST_API_KEY.to_string(),
        })
    }

    pub async fn ensure_model(&self, model: &str) -> LlmResult<()> {
        pull_model(&self.base_url, model).await
    }
}

fn host_ollama_models_dir() -> LlmResult<PathBuf> {
    let cwd = std::env::current_dir().map_err(|err| Error::Internal {
        message: err.to_string(),
    })?;
    let path = cwd.join(OLLAMA_MODELS_DIR_RELATIVE);
    std::fs::create_dir_all(&path).map_err(|err| Error::Internal {
        message: err.to_string(),
    })?;
    Ok(path)
}

async fn wait_until_ready(base_url: &str) -> LlmResult<()> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}{TAGS_PATH}");
    info!(
        target: "borg_llm_test",
        url = url.as_str(),
        max_attempts = MAX_READINESS_ATTEMPTS,
        "waiting for ollama readiness"
    );
    for attempt in 1..=MAX_READINESS_ATTEMPTS {
        let response = client.get(&url).send().await;
        if let Ok(response) = response {
            if response.status().is_success() {
                info!(
                    target: "borg_llm_test",
                    attempt,
                    "ollama readiness probe succeeded"
                );
                return Ok(());
            }
            trace!(
                target: "borg_llm_test",
                attempt,
                status = %response.status(),
                "ollama readiness probe returned non-success status"
            );
        } else if let Err(err) = &response {
            trace!(
                target: "borg_llm_test",
                attempt,
                error = %err,
                "ollama readiness probe failed"
            );
        }
        sleep(Duration::from_millis(READINESS_BACKOFF_MILLIS)).await;
    }
    error!(
        target: "borg_llm_test",
        url = url.as_str(),
        max_attempts = MAX_READINESS_ATTEMPTS,
        "ollama readiness timed out"
    );

    Err(Error::Internal {
        message: format!("ollama container never became ready at {}", url),
    })
}

async fn pull_model(base_url: &str, model: &str) -> LlmResult<()> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}{PULL_PATH}");
    info!(
        target: "borg_llm_test",
        model,
        url = url.as_str(),
        max_attempts = MAX_PULL_ATTEMPTS,
        "pulling ollama model for test"
    );
    for attempt in 1..=MAX_PULL_ATTEMPTS {
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
                info!(
                    target: "borg_llm_test",
                    model,
                    attempt,
                    "ollama model pull completed"
                );
                return Ok(());
            }
            debug!(
                target: "borg_llm_test",
                model,
                attempt,
                status = %response.status(),
                "ollama model pull returned non-success status"
            );
        } else if let Err(err) = &response {
            trace!(
                target: "borg_llm_test",
                model,
                attempt,
                error = %err,
                "ollama model pull request failed"
            );
        }
        sleep(Duration::from_millis(PULL_BACKOFF_MILLIS)).await;
    }
    error!(
        target: "borg_llm_test",
        model,
        url = url.as_str(),
        max_attempts = MAX_PULL_ATTEMPTS,
        "ollama model pull timed out"
    );

    Err(Error::Internal {
        message: format!("ollama model pull never completed for {} at {}", model, url),
    })
}
