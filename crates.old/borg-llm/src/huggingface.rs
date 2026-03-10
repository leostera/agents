use std::path::{Component, Path, PathBuf};

use borg_core::borgdir::BorgDir;
use reqwest::{Client, Url};
use serde::Deserialize;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::{LlmError, Result};

const DEFAULT_HF_BASE_URL: &str = "https://huggingface.co";
const DEFAULT_REVISION: &str = "main";
const HF_PROVIDER_NAME: &str = "huggingface";

#[derive(Debug, Clone)]
pub struct DownloadGgufRequest {
    pub repo_id: String,
    pub revision: String,
    pub filename: Option<String>,
    pub auth_token: Option<String>,
    pub overwrite: bool,
}

impl DownloadGgufRequest {
    pub fn new(repo_id: impl Into<String>) -> Self {
        Self {
            repo_id: repo_id.into(),
            revision: DEFAULT_REVISION.to_string(),
            filename: None,
            auth_token: None,
            overwrite: false,
        }
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = revision.into();
        self
    }

    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn with_auth_token(mut self, auth_token: impl Into<String>) -> Self {
        self.auth_token = Some(auth_token.into());
        self
    }

    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DownloadedGguf {
    pub repo_id: String,
    pub revision: String,
    pub filename: String,
    pub local_path: PathBuf,
    pub bytes_downloaded: u64,
    pub downloaded: bool,
}

#[derive(Debug, Clone)]
pub struct HuggingFaceGgufDownloader {
    client: Client,
    base_url: String,
    models_root: PathBuf,
}

impl Default for HuggingFaceGgufDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl HuggingFaceGgufDownloader {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DEFAULT_HF_BASE_URL.to_string(),
            models_root: default_models_root(),
        }
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_models_root(mut self, models_root: impl Into<PathBuf>) -> Self {
        self.models_root = models_root.into();
        self
    }

    pub fn models_root(&self) -> &Path {
        &self.models_root
    }

    pub async fn download_gguf(&self, request: &DownloadGgufRequest) -> Result<DownloadedGguf> {
        let repo = parse_repo_ref(&request.repo_id)?;
        let revision = normalize_non_empty("revision", &request.revision)?;
        let metadata = self
            .fetch_model_metadata(&repo, request.auth_token.as_deref())
            .await?;
        let filename = select_filename(&metadata, request.filename.as_deref())?;
        let relative_filename = safe_relative_path(&filename)?;
        let destination = self.destination_path(&repo, revision, &relative_filename);

        if fs::try_exists(&destination).await? && !request.overwrite {
            let bytes_downloaded = fs::metadata(&destination).await.map(|meta| meta.len())?;
            return Ok(DownloadedGguf {
                repo_id: request.repo_id.clone(),
                revision: revision.to_string(),
                filename,
                local_path: destination,
                bytes_downloaded,
                downloaded: false,
            });
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).await?;
        }

        let download_url = self.download_url(&repo, revision, &relative_filename)?;
        debug!(
            repo_id = request.repo_id.as_str(),
            revision,
            filename = filename.as_str(),
            destination = %destination.display(),
            "downloading gguf model from huggingface"
        );

        let response = with_auth(self.client.get(download_url), request.auth_token.as_deref())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(LlmError::ProviderHttp {
                provider: HF_PROVIDER_NAME,
                capability: "download-gguf",
                status: response.status().as_u16(),
            });
        }

        let partial_path = destination.with_extension("part");
        let mut file = fs::File::create(&partial_path).await?;
        let mut response = response;
        let mut bytes_downloaded = 0u64;
        while let Some(chunk) = response.chunk().await? {
            bytes_downloaded += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);
        fs::rename(&partial_path, &destination).await?;

        Ok(DownloadedGguf {
            repo_id: request.repo_id.clone(),
            revision: revision.to_string(),
            filename,
            local_path: destination,
            bytes_downloaded,
            downloaded: true,
        })
    }

    async fn fetch_model_metadata(
        &self,
        repo: &RepoRef,
        auth_token: Option<&str>,
    ) -> Result<ModelMetadata> {
        let endpoint = self.metadata_url(repo)?;
        let response = with_auth(self.client.get(endpoint), auth_token)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(LlmError::ProviderHttp {
                provider: HF_PROVIDER_NAME,
                capability: "model-metadata",
                status: response.status().as_u16(),
            });
        }
        Ok(response.json::<ModelMetadata>().await?)
    }

    fn destination_path(
        &self,
        repo: &RepoRef,
        revision: &str,
        relative_filename: &Path,
    ) -> PathBuf {
        self.models_root
            .join(&repo.organization)
            .join(&repo.model)
            .join(revision)
            .join(relative_filename)
    }

    fn metadata_url(&self, repo: &RepoRef) -> Result<Url> {
        let mut url = Url::parse(&self.base_url).map_err(|error| {
            LlmError::configuration(format!("invalid Hugging Face base URL: {error}"))
        })?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| LlmError::configuration("invalid Hugging Face base URL path"))?;
            segments.push("api");
            segments.push("models");
            segments.push(&repo.organization);
            segments.push(&repo.model);
        }
        Ok(url)
    }

    fn download_url(
        &self,
        repo: &RepoRef,
        revision: &str,
        relative_filename: &Path,
    ) -> Result<Url> {
        let mut url = Url::parse(&self.base_url).map_err(|error| {
            LlmError::configuration(format!("invalid Hugging Face base URL: {error}"))
        })?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| LlmError::configuration("invalid Hugging Face base URL path"))?;
            segments.push(&repo.organization);
            segments.push(&repo.model);
            segments.push("resolve");
            segments.push(revision);
            for component in relative_filename.components() {
                if let Component::Normal(part) = component {
                    segments.push(&part.to_string_lossy());
                }
            }
        }
        url.query_pairs_mut().append_pair("download", "true");
        Ok(url)
    }
}

fn with_auth(
    builder: reqwest::RequestBuilder,
    auth_token: Option<&str>,
) -> reqwest::RequestBuilder {
    match auth_token.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    }
}

fn default_models_root() -> PathBuf {
    BorgDir::new().root().join("models")
}

#[derive(Debug, Clone)]
struct RepoRef {
    organization: String,
    model: String,
}

fn parse_repo_ref(repo_id: &str) -> Result<RepoRef> {
    let (organization, model) = repo_id
        .trim()
        .split_once('/')
        .ok_or_else(|| LlmError::configuration("repo_id must use '<org>/<model>' format"))?;
    let organization = normalize_repo_segment("organization", organization)?;
    let model = normalize_repo_segment("model", model)?;
    Ok(RepoRef {
        organization: organization.to_string(),
        model: model.to_string(),
    })
}

fn normalize_repo_segment<'a>(name: &str, segment: &'a str) -> Result<&'a str> {
    let segment = normalize_non_empty(name, segment)?;
    if segment.contains('/') || segment.contains('\\') {
        return Err(LlmError::configuration(format!(
            "{name} in repo_id must not contain path separators",
        )));
    }
    Ok(segment)
}

fn normalize_non_empty<'a>(name: &str, value: &'a str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LlmError::configuration(format!("{name} must not be empty")));
    }
    Ok(value)
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path.trim());
    if path.is_absolute() {
        return Err(LlmError::configuration(
            "gguf filename must be a relative path",
        ));
    }

    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => relative.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(LlmError::configuration(
                    "gguf filename must not traverse parent directories",
                ));
            }
        }
    }

    if relative.as_os_str().is_empty() {
        return Err(LlmError::configuration("gguf filename must not be empty"));
    }
    Ok(relative)
}

fn select_filename(metadata: &ModelMetadata, requested_filename: Option<&str>) -> Result<String> {
    if let Some(filename) = requested_filename {
        let filename = filename.trim();
        if filename.is_empty() {
            return Err(LlmError::configuration("filename must not be empty"));
        }
        let exists_in_repo = metadata
            .siblings
            .iter()
            .any(|sibling| sibling.rfilename.trim() == filename);
        if !exists_in_repo {
            return Err(LlmError::configuration(format!(
                "requested filename `{filename}` was not found in Hugging Face model repository",
            )));
        }
        return Ok(filename.to_string());
    }

    let mut gguf_files = metadata
        .siblings
        .iter()
        .map(|sibling| sibling.rfilename.trim())
        .filter(|name| name.to_ascii_lowercase().ends_with(".gguf"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    gguf_files.sort();
    gguf_files.dedup();

    match gguf_files.len() {
        0 => Err(LlmError::configuration(
            "no .gguf files found in Hugging Face model repository",
        )),
        1 => Ok(gguf_files.remove(0)),
        _ => Err(LlmError::configuration(format!(
            "multiple .gguf files found; specify filename explicitly: {}",
            gguf_files.join(", ")
        ))),
    }
}

#[derive(Debug, Deserialize)]
struct ModelMetadata {
    #[serde(default)]
    siblings: Vec<ModelSibling>,
}

#[derive(Debug, Deserialize)]
struct ModelSibling {
    rfilename: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_ref_requires_org_and_model() {
        let err = parse_repo_ref("tinyllama").expect_err("expected error");
        assert!(
            err.to_string()
                .contains("repo_id must use '<org>/<model>' format"),
            "actual: {err}"
        );
    }

    #[test]
    fn parse_repo_ref_parses_valid_repo() {
        let parsed = parse_repo_ref("TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF").expect("valid repo");
        assert_eq!(parsed.organization, "TheBloke");
        assert_eq!(parsed.model, "TinyLlama-1.1B-Chat-v1.0-GGUF");
    }

    #[test]
    fn safe_relative_path_rejects_parent_segments() {
        let err = safe_relative_path("../model.gguf").expect_err("expected error");
        assert!(
            err.to_string()
                .contains("gguf filename must not traverse parent directories"),
            "actual: {err}"
        );
    }

    #[test]
    fn select_filename_requires_explicit_name_when_multiple_ggufs_exist() {
        let metadata = ModelMetadata {
            siblings: vec![
                ModelSibling {
                    rfilename: "model-q4.gguf".to_string(),
                },
                ModelSibling {
                    rfilename: "model-q8.gguf".to_string(),
                },
            ],
        };
        let err = select_filename(&metadata, None).expect_err("expected error");
        assert!(
            err.to_string()
                .contains("multiple .gguf files found; specify filename explicitly"),
            "actual: {err}"
        );
    }

    #[test]
    fn select_filename_uses_single_gguf_when_present() {
        let metadata = ModelMetadata {
            siblings: vec![
                ModelSibling {
                    rfilename: "README.md".to_string(),
                },
                ModelSibling {
                    rfilename: "tinyllama.Q4_K_M.gguf".to_string(),
                },
            ],
        };
        let selected = select_filename(&metadata, None).expect("gguf should be selected");
        assert_eq!(selected, "tinyllama.Q4_K_M.gguf");
    }

    #[test]
    fn destination_path_includes_repo_and_revision() {
        let downloader =
            HuggingFaceGgufDownloader::new().with_models_root("/tmp/borg-models-for-test");
        let repo = RepoRef {
            organization: "TheBloke".to_string(),
            model: "TinyLlama-1.1B-Chat-v1.0-GGUF".to_string(),
        };
        let path = downloader.destination_path(
            &repo,
            "main",
            Path::new("tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf"),
        );
        assert_eq!(
            path,
            PathBuf::from(
                "/tmp/borg-models-for-test/TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF/main/tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf"
            )
        );
    }
}
