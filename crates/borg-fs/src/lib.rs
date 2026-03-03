mod local;

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use base64::Engine;
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain, ToolchainBuilder};
use borg_core::Uri;
use borg_db::{BorgDb, FileRecord};
use serde_json::{Value, json};
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

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "audio" => Some(Self::Audio),
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            _ => None,
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
    fn root_path(&self) -> Option<String> {
        None
    }

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

    pub async fn list_files(
        &self,
        limit: usize,
        query: Option<&str>,
        include_deleted: bool,
    ) -> Result<Vec<FileRecord>> {
        self.db.list_files(limit, query, include_deleted).await
    }

    pub async fn count_files(&self, include_deleted: bool) -> Result<usize> {
        self.db.count_files(include_deleted).await
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.backend_name()
    }

    pub fn root_path(&self) -> Option<String> {
        self.backend.root_path()
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

pub fn default_borg_fs_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "BorgFS-ls".to_string(),
            description: "List BorgFS files with optional text query filters".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 10000},
                    "q": {"type": "string"},
                    "query": {"type": "string"},
                    "includeDeleted": {"type": "boolean"},
                    "include_deleted": {"type": "boolean"}
                }
            }),
        },
        ToolSpec {
            name: "BorgFS-search".to_string(),
            description: "Search BorgFS files by query string".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "q": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 10000},
                    "includeDeleted": {"type": "boolean"},
                    "include_deleted": {"type": "boolean"}
                }
            }),
        },
        ToolSpec {
            name: "BorgFS-get".to_string(),
            description: "Get a BorgFS file by file_id and return base64 content".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["file_id"],
                "properties": {
                    "file_id": {"type": "string"}
                }
            }),
        },
        ToolSpec {
            name: "BorgFS-put".to_string(),
            description: "Store bytes in BorgFS using base64 content and return file metadata"
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["content_base64", "session_id"],
                "properties": {
                    "kind": {"type": "string", "enum": ["audio", "image", "video"]},
                    "content_base64": {"type": "string"},
                    "session_id": {"type": "string"}
                }
            }),
        },
        ToolSpec {
            name: "BorgFS-delete".to_string(),
            description: "Soft-delete a BorgFS file by file_id".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["file_id"],
                "properties": {
                    "file_id": {"type": "string"}
                }
            }),
        },
        ToolSpec {
            name: "BorgFS-settings".to_string(),
            description: "Read BorgFS backend settings and object counters".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

pub fn build_borg_fs_toolchain(fs: BorgFs) -> Result<Toolchain> {
    let mut builder = ToolchainBuilder::new();
    for spec in default_borg_fs_tool_specs() {
        let tool_name = spec.name.clone();
        let fs = fs.clone();
        let tool = Tool::new(spec, None, move |request| {
            let fs = fs.clone();
            let tool_name = tool_name.clone();
            async move {
                let output = run_borg_fs_tool(&fs, &tool_name, &request.arguments).await?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&output)?),
                })
            }
        });
        builder = builder.add_tool(tool)?;
    }
    builder.build()
}

pub async fn run_borg_fs_tool(fs: &BorgFs, tool_name: &str, arguments: &Value) -> Result<Value> {
    match tool_name {
        "BorgFS-ls" => {
            let limit = read_u64(arguments, &["limit"])
                .map(|value| usize::try_from(value).unwrap_or(500))
                .unwrap_or(500)
                .clamp(1, 10_000);
            let query = read_string(arguments, &["q", "query"]);
            let include_deleted = read_bool(arguments, &["includeDeleted", "include_deleted"]);
            let files = fs
                .list_files(limit, query.as_deref(), include_deleted)
                .await?;
            Ok(json!({ "files": files }))
        }
        "BorgFS-search" => {
            let limit = read_u64(arguments, &["limit"])
                .map(|value| usize::try_from(value).unwrap_or(500))
                .unwrap_or(500)
                .clamp(1, 10_000);
            let query = read_string(arguments, &["query", "q"])
                .ok_or_else(|| anyhow!("query is required"))?;
            let include_deleted = read_bool(arguments, &["includeDeleted", "include_deleted"]);
            let files = fs.list_files(limit, Some(&query), include_deleted).await?;
            Ok(json!({ "files": files }))
        }
        "BorgFS-get" => {
            let file_id = read_string(arguments, &["file_id"])
                .ok_or_else(|| anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let (record, bytes) = fs.read_all(&file_id).await?;
            Ok(json!({
                "file": record,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }))
        }
        "BorgFS-put" => {
            let kind = read_string(arguments, &["kind"])
                .and_then(|value| FileKind::parse(&value))
                .unwrap_or(FileKind::Audio);
            let session_id = read_string(arguments, &["session_id"])
                .ok_or_else(|| anyhow!("session_id is required"))?;
            let session_id = Uri::parse(&session_id)?;
            let content = read_string(arguments, &["content_base64"])
                .ok_or_else(|| anyhow!("content_base64 is required"))?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(content)?;
            let file = fs
                .put_bytes(kind, &bytes, PutFileMetadata { session_id })
                .await?;
            Ok(json!({ "file": file }))
        }
        "BorgFS-delete" => {
            let file_id = read_string(arguments, &["file_id"])
                .ok_or_else(|| anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let deleted = fs.soft_delete(&file_id).await?;
            Ok(json!({ "deleted": deleted }))
        }
        "BorgFS-settings" => {
            let total = fs.count_files(true).await?;
            let active = fs.count_files(false).await?;
            Ok(json!({
                "backend": fs.backend_name(),
                "root_path": fs.root_path(),
                "counts": {
                    "total": total,
                    "active": active,
                    "deleted": total.saturating_sub(active),
                }
            }))
        }
        _ => Err(anyhow!("unknown BorgFS tool: {tool_name}")),
    }
}

fn read_string(arguments: &Value, keys: &[&str]) -> Option<String> {
    let object = arguments.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn read_bool(arguments: &Value, keys: &[&str]) -> bool {
    let Some(object) = arguments.as_object() else {
        return false;
    };
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_bool) {
            return value;
        }
    }
    false
}

fn read_u64(arguments: &Value, keys: &[&str]) -> Option<u64> {
    let object = arguments.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_u64) {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
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

    #[tokio::test]
    async fn borgfs_tools_put_get_and_settings_roundtrip() -> Result<()> {
        let db_path = tmp_db_path("tools");
        let db =
            BorgDb::open_local(db_path.to_str().ok_or_else(|| anyhow!("invalid db path"))?).await?;
        db.migrate().await?;
        let fs = BorgFs::local(db, tmp_root("tools"));

        let put = run_borg_fs_tool(
            &fs,
            "BorgFS-put",
            &json!({
                "kind": "audio",
                "session_id": "borg:session:tools",
                "content_base64": base64::engine::general_purpose::STANDARD.encode("hello-tools")
            }),
        )
        .await?;
        let file_id = put
            .get("file")
            .and_then(|value| value.get("file_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing file_id"))?;

        let get = run_borg_fs_tool(&fs, "BorgFS-get", &json!({ "file_id": file_id })).await?;
        let content = get
            .get("content_base64")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing content_base64"))?;
        assert_eq!(
            base64::engine::general_purpose::STANDARD.decode(content)?,
            b"hello-tools"
        );

        let settings = run_borg_fs_tool(&fs, "BorgFS-settings", &json!({})).await?;
        assert_eq!(settings["counts"]["total"], json!(1));
        assert_eq!(settings["counts"]["active"], json!(1));

        Ok(())
    }
}
