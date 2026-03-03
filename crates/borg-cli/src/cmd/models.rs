use anyhow::Result;
use borg_llm::huggingface::{DownloadGgufRequest, HuggingFaceGgufDownloader};
use clap::Subcommand;
use serde_json::json;

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum ModelsCommand {
    #[command(about = "Download and cache a GGUF model from Hugging Face")]
    Pull {
        #[arg(help = "Hugging Face repo id in <org>/<model> format")]
        repo_id: String,
        #[arg(
            long,
            default_value = "main",
            help = "Repository revision/branch/tag/commit"
        )]
        revision: String,
        #[arg(
            long,
            help = "GGUF filename to download when repository has multiple .gguf files"
        )]
        file: Option<String>,
        #[arg(long, help = "Hugging Face token")]
        token: Option<String>,
        #[arg(long, help = "Overwrite cached local file if it already exists")]
        overwrite: bool,
    },
}

pub async fn run(_app: &BorgCliApp, cmd: ModelsCommand) -> Result<()> {
    let output = match cmd {
        ModelsCommand::Pull {
            repo_id,
            revision,
            file,
            token,
            overwrite,
        } => {
            let request = DownloadGgufRequest::new(repo_id.clone())
                .with_revision(revision.clone())
                .with_overwrite(overwrite);
            let request = if let Some(file) = file {
                request.with_filename(file)
            } else {
                request
            };
            let request = if let Some(token) = token {
                request.with_auth_token(token)
            } else {
                request
            };

            let downloader = HuggingFaceGgufDownloader::new();
            let downloaded = downloader.download_gguf(&request).await?;
            json!({
                "ok": true,
                "entity": "models",
                "item": {
                    "provider": "huggingface",
                    "repo_id": downloaded.repo_id,
                    "revision": downloaded.revision,
                    "filename": downloaded.filename,
                    "local_path": downloaded.local_path,
                    "bytes_downloaded": downloaded.bytes_downloaded,
                    "downloaded": downloaded.downloaded,
                }
            })
        }
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
