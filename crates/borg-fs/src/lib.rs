mod local;

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_core::Uri;
use borg_db::{BorgDb, FileRecord};
use serde_json::json;
use tokio::io::{AsyncRead, AsyncReadExt};

pub use local::LocalFsBackend;

const DEFAULT_MIME_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Audio,
    Image,
    Video,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PutFileMetadata {
    pub session_id: Uri,
}

pub struct FileReadHandle {
    pub record: FileRecord,
    pub reader: Pin<Box<dyn AsyncRead + Send>>,
}

pub struct BackendPutResult {
    pub storage_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha512: String,
}

#[async_trait]
pub trait BorgFsBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;

    async fn put_reader(
        &self,
        kind: FileKind,
        reader: &mut (dyn AsyncRead + Unpin + Send),
    ) -> Result<BackendPutResult>;

    async fn open_reader(&self, storage_key: &str) -> Result<Pin<Box<dyn AsyncRead + Send>>>;
}

#[derive(Clone)]
pub struct BorgFs {
    db: BorgDb,
    backend: Arc<dyn BorgFsBackend>,
}

impl BorgFs {
    pub fn new(db: BorgDb, backend: Arc<dyn BorgFsBackend>) -> Self {
        Self { db, backend }
    }

    pub fn local(db: BorgDb, root: PathBuf) -> Self {
        Self::new(db, Arc::new(LocalFsBackend::new(root)))
    }

    pub async fn put_reader(
        &self,
        kind: FileKind,
        reader: &mut (dyn AsyncRead + Unpin + Send),
        metadata: PutFileMetadata,
    ) -> Result<FileRecord> {
        let put_result = self.backend.put_reader(kind, reader).await?;
        let file_id = Uri::from_parts("borg", kind.as_str(), Some(&put_result.sha512))?;
        let metadata_json = json!({
            "session_id": metadata.session_id.as_str(),
        });
        self.db
            .upsert_file(
                &file_id,
                self.backend.backend_name(),
                &put_result.storage_key,
                normalize_content_type(&put_result.content_type),
                put_result.size_bytes,
                &put_result.sha512,
                None,
                &metadata_json,
            )
            .await
    }

    pub async fn put_bytes(
        &self,
        kind: FileKind,
        bytes: &[u8],
        metadata: PutFileMetadata,
    ) -> Result<FileRecord> {
        let mut cursor = bytes;
        self.put_reader(kind, &mut cursor, metadata).await
    }

    pub async fn get(&self, file_id: &Uri) -> Result<FileReadHandle> {
        let Some(record) = self.db.get_file(file_id).await? else {
            return Err(anyhow!("file not found: {}", file_id));
        };
        if record.deleted_at.is_some() {
            return Err(anyhow!("file not found: {}", file_id));
        }
        let reader = self.backend.open_reader(&record.storage_key).await?;
        Ok(FileReadHandle { record, reader })
    }

    pub async fn read_all(&self, file_id: &Uri) -> Result<(FileRecord, Vec<u8>)> {
        let mut handle = self.get(file_id).await?;
        let mut out = Vec::new();
        handle.reader.read_to_end(&mut out).await?;
        Ok((handle.record, out))
    }

    pub async fn exists(&self, file_id: &Uri) -> Result<bool> {
        self.db.file_exists(file_id).await
    }

    pub async fn soft_delete(&self, file_id: &Uri) -> Result<u64> {
        self.db.soft_delete_file(file_id).await
    }
}

fn normalize_content_type(content_type: &str) -> &str {
    let trimmed = content_type.trim();
    if trimmed.is_empty() {
        DEFAULT_MIME_TYPE
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("borg-fs-{test_name}-{}.db", uuid::Uuid::new_v4()));
        path
    }

    fn tmp_root(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("borg-fs-root-{test_name}-{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn put_get_soft_delete_roundtrip() -> Result<()> {
        let db_path = tmp_db_path("roundtrip");
        let db =
            BorgDb::open_local(db_path.to_str().ok_or_else(|| anyhow!("invalid db path"))?).await?;
        db.migrate().await?;
        let fs = BorgFs::local(db.clone(), tmp_root("roundtrip"));

        let session_id = Uri::from_parts("borg", "session", Some("s1"))?;
        let first = fs
            .put_bytes(
                FileKind::Audio,
                b"hello-audio",
                PutFileMetadata {
                    session_id: session_id.clone(),
                },
            )
            .await?;
        let second = fs
            .put_bytes(
                FileKind::Audio,
                b"hello-audio",
                PutFileMetadata { session_id },
            )
            .await?;

        assert_eq!(first.file_id, second.file_id);

        let (record, bytes) = fs.read_all(&first.file_id).await?;
        assert_eq!(record.file_id, first.file_id);
        assert_eq!(bytes, b"hello-audio");
        assert!(fs.exists(&first.file_id).await?);

        let deleted = fs.soft_delete(&first.file_id).await?;
        assert_eq!(deleted, 1);
        assert!(!fs.exists(&first.file_id).await?);
        assert!(fs.get(&first.file_id).await.is_err());

        Ok(())
    }
}
