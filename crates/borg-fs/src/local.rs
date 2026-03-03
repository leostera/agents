use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha512};
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::{BackendPutResult, BorgFsBackend, FileKind};

const SNIFF_BYTES: usize = 16 * 1024;
const BUFFER_SIZE: usize = 8 * 1024;
const DEFAULT_MIME_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone)]
pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl BorgFsBackend for LocalFsBackend {
    fn backend_name(&self) -> &'static str {
        "local"
    }

    async fn put_reader(
        &self,
        kind: FileKind,
        reader: &mut (dyn AsyncRead + Unpin + Send),
    ) -> Result<BackendPutResult> {
        fs::create_dir_all(&self.root).await?;

        let tmp_dir = self.root.join("tmp");
        fs::create_dir_all(&tmp_dir).await?;

        let tmp_name = format!("{}.upload", Uuid::new_v4());
        let tmp_path = tmp_dir.join(tmp_name);
        let mut tmp_file = fs::File::create(&tmp_path).await?;

        let mut hasher = Sha512::new();
        let mut sniff = Vec::with_capacity(SNIFF_BYTES);
        let mut total_bytes: i64 = 0;
        let mut buf = [0_u8; BUFFER_SIZE];

        loop {
            let read = reader.read(&mut buf).await?;
            if read == 0 {
                break;
            }
            total_bytes += i64::try_from(read).unwrap_or(0);
            hasher.update(&buf[..read]);
            if sniff.len() < SNIFF_BYTES {
                let remaining = SNIFF_BYTES - sniff.len();
                sniff.extend_from_slice(&buf[..read.min(remaining)]);
            }
            tmp_file.write_all(&buf[..read]).await?;
        }

        tmp_file.flush().await?;

        let sha512 = hex::encode(hasher.finalize());
        let shard_a = &sha512[..2];
        let shard_b = &sha512[2..4];
        let storage_key = format!("{}/{}/{}/{}", kind.as_str(), shard_a, shard_b, sha512);
        let final_path = self.root.join(&storage_key);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if fs::metadata(&final_path).await.is_ok() {
            let _ = fs::remove_file(&tmp_path).await;
        } else {
            fs::rename(&tmp_path, &final_path).await?;
        }

        let content_type = infer::get(&sniff)
            .map(|kind| kind.mime_type().to_string())
            .unwrap_or_else(|| DEFAULT_MIME_TYPE.to_string());

        Ok(BackendPutResult {
            storage_key,
            content_type,
            size_bytes: total_bytes,
            sha512,
        })
    }

    async fn open_reader(&self, storage_key: &str) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
        let path = self.root.join(storage_key);
        let file = fs::File::open(path).await?;
        Ok(Box::pin(file))
    }
}
