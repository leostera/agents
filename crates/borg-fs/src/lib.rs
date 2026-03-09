mod local;

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use base64::Engine;
use borg_agent::{
    Tool, ToolCall, ToolResponse, ToolResult, ToolResultData, ToolSpec, Toolchain, ToolchainBuilder,
};
use borg_core::Uri;
use borg_db::{BorgDb, FileRecord};
use serde::{Deserialize, Serialize};
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
    pub actor_id: Uri,
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
            "actor_id": metadata.actor_id.as_str(),
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
                "required": ["content_base64", "actor_id"],
                "properties": {
                    "kind": {"type": "string", "enum": ["audio", "image", "video"]},
                    "content_base64": {"type": "string"},
                    "actor_id": {"type": "string"}
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

pub fn build_borg_fs_toolchain<TToolCall, TToolResult>(
    fs: BorgFs,
) -> Result<Toolchain<TToolCall, TToolResult>>
where
    TToolCall: ToolCall,
    TToolResult: ToolResult,
{
    let mut builder = ToolchainBuilder::new();
    for spec in default_borg_fs_tool_specs() {
        let tool_name = spec.name.clone();
        let fs = fs.clone();
        let tool = Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<FsToolArgs>| {
                let fs = fs.clone();
                let tool_name = tool_name.clone();
                async move {
                    let output = run_borg_fs_tool(&fs, &tool_name, &request.arguments).await?;
                    Ok(ToolResponse {
                        output: ToolResultData::Ok(TToolResult::from(serde_json::to_value(
                            output,
                        )?)),
                    })
                }
            },
        );
        builder = builder.add_tool(tool)?;
    }
    builder.build()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FsToolArgs {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    #[serde(rename = "includeDeleted")]
    include_deleted_camel: Option<bool>,
    #[serde(default)]
    include_deleted: Option<bool>,
    #[serde(default)]
    file_id: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    actor_id: Option<String>,
    #[serde(default)]
    content_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FsToolCounts {
    pub total: usize,
    pub active: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FsToolOutput {
    Files {
        files: Vec<FileRecord>,
    },
    Get {
        file: FileRecord,
        content_base64: String,
    },
    Put {
        file: FileRecord,
    },
    Delete {
        deleted: u64,
    },
    Settings {
        backend: String,
        root_path: Option<String>,
        counts: FsToolCounts,
    },
}

impl FsToolArgs {
    fn query(&self) -> Option<String> {
        option_non_empty(self.query.clone()).or_else(|| option_non_empty(self.q.clone()))
    }

    fn include_deleted(&self) -> bool {
        self.include_deleted_camel
            .or(self.include_deleted)
            .unwrap_or(false)
    }

    fn normalized_limit(&self) -> usize {
        self.limit
            .map(|value| usize::try_from(value).unwrap_or(500))
            .unwrap_or(500)
            .clamp(1, 10_000)
    }
}

pub async fn run_borg_fs_tool(
    fs: &BorgFs,
    tool_name: &str,
    arguments: &FsToolArgs,
) -> Result<FsToolOutput> {
    match tool_name {
        "BorgFS-ls" => {
            let files = fs
                .list_files(
                    arguments.normalized_limit(),
                    arguments.query().as_deref(),
                    arguments.include_deleted(),
                )
                .await?;
            Ok(FsToolOutput::Files { files })
        }
        "BorgFS-search" => {
            let query = arguments
                .query()
                .ok_or_else(|| anyhow!("query is required"))?;
            let files = fs
                .list_files(
                    arguments.normalized_limit(),
                    Some(&query),
                    arguments.include_deleted(),
                )
                .await?;
            Ok(FsToolOutput::Files { files })
        }
        "BorgFS-get" => {
            let file_id = option_non_empty(arguments.file_id.clone())
                .ok_or_else(|| anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let (record, bytes) = fs.read_all(&file_id).await?;
            Ok(FsToolOutput::Get {
                file: record,
                content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            })
        }
        "BorgFS-put" => {
            let kind = option_non_empty(arguments.kind.clone())
                .and_then(|value| FileKind::parse(&value))
                .unwrap_or(FileKind::Audio);
            let actor_id = option_non_empty(arguments.actor_id.clone())
                .ok_or_else(|| anyhow!("actor_id is required"))?;
            let actor_id = Uri::parse(&actor_id)?;
            let content = option_non_empty(arguments.content_base64.clone())
                .ok_or_else(|| anyhow!("content_base64 is required"))?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(content)?;
            let file = fs
                .put_bytes(kind, &bytes, PutFileMetadata { actor_id })
                .await?;
            Ok(FsToolOutput::Put { file })
        }
        "BorgFS-delete" => {
            let file_id = option_non_empty(arguments.file_id.clone())
                .ok_or_else(|| anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let deleted = fs.soft_delete(&file_id).await?;
            Ok(FsToolOutput::Delete { deleted })
        }
        "BorgFS-settings" => {
            let total = fs.count_files(true).await?;
            let active = fs.count_files(false).await?;
            Ok(FsToolOutput::Settings {
                backend: fs.backend_name().to_string(),
                root_path: fs.root_path(),
                counts: FsToolCounts {
                    total,
                    active,
                    deleted: total.saturating_sub(active),
                },
            })
        }
        _ => Err(anyhow!("unknown BorgFS tool: {tool_name}")),
    }
}

fn option_non_empty(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

        let actor_id = Uri::from_parts("borg", "actor", Some("s1"))?;
        let first = fs
            .put_bytes(
                FileKind::Audio,
                b"hello-audio",
                PutFileMetadata {
                    actor_id: actor_id.clone(),
                },
            )
            .await?;
        let second = fs
            .put_bytes(
                FileKind::Audio,
                b"hello-audio",
                PutFileMetadata { actor_id },
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
            &FsToolArgs {
                kind: Some("audio".to_string()),
                actor_id: Some("borg:actor:tools".to_string()),
                content_base64: Some(
                    base64::engine::general_purpose::STANDARD.encode("hello-tools"),
                ),
                ..Default::default()
            },
        )
        .await?;
        let file_id = match put {
            FsToolOutput::Put { file } => file.file_id.to_string(),
            other => return Err(anyhow!("unexpected output variant: {:?}", other)),
        };

        let get = run_borg_fs_tool(
            &fs,
            "BorgFS-get",
            &FsToolArgs {
                file_id: Some(file_id),
                ..Default::default()
            },
        )
        .await?;
        let content = match get {
            FsToolOutput::Get { content_base64, .. } => content_base64,
            other => return Err(anyhow!("unexpected output variant: {:?}", other)),
        };
        assert_eq!(
            base64::engine::general_purpose::STANDARD.decode(&content)?,
            b"hello-tools"
        );

        let settings = run_borg_fs_tool(&fs, "BorgFS-settings", &FsToolArgs::default()).await?;
        match settings {
            FsToolOutput::Settings { counts, .. } => {
                assert_eq!(counts.total, 1);
                assert_eq!(counts.active, 1);
            }
            other => return Err(anyhow!("unexpected output variant: {:?}", other)),
        }

        Ok(())
    }
}
