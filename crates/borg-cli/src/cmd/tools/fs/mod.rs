use anyhow::Result;
use base64::Engine;
use borg_core::Uri;
use borg_fs::{FileKind, PutFileMetadata};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Map, Value, json};

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum FsToolsCommand {
    #[command(about = "List FS commands")]
    List,
    #[command(about = "List files")]
    Ls(LsArgs),
    #[command(about = "Search files by query")]
    Search(SearchArgs),
    #[command(about = "Get one file by file_id")]
    Get(GetArgs),
    #[command(about = "Put one file using base64 content")]
    Put(PutArgs),
    #[command(about = "Soft-delete one file by file_id")]
    Delete(DeleteArgs),
    #[command(about = "Show BorgFS backend settings and counters")]
    Settings,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum FileKindArg {
    Audio,
    Image,
    Video,
}

impl FileKindArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

#[derive(Args, Debug)]
pub struct RawPayloadArg {
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub include_deleted: Option<bool>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub include_deleted: Option<bool>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct GetArgs {
    #[arg(long)]
    pub file_id: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct PutArgs {
    #[arg(long, value_enum)]
    pub kind: Option<FileKindArg>,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long)]
    pub content_base64: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    #[arg(long)]
    pub file_id: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

pub fn command_names() -> Vec<&'static str> {
    vec!["ls", "search", "get", "put", "delete", "settings"]
}

pub async fn run(app: &BorgCliApp, cmd: FsToolsCommand) -> Result<Value> {
    let fs = app.open_borg_fs().await?;
    match cmd {
        FsToolsCommand::List => Ok(json!({ "commands": command_names() })),
        FsToolsCommand::Ls(args) => {
            let mut map = Map::new();
            insert_opt_u64(&mut map, "limit", args.limit);
            insert_opt_string(&mut map, "query", args.query);
            insert_opt_bool(&mut map, "include_deleted", args.include_deleted);
            let payload = payload(args.raw.payload_json, map)?;
            let limit = read_u64(&payload, &["limit"])
                .map(|value| usize::try_from(value).unwrap_or(500))
                .unwrap_or(500)
                .clamp(1, 10_000);
            let query = read_string(&payload, &["query", "q"]);
            let include_deleted = read_bool(&payload, &["include_deleted", "includeDeleted"]);
            let files = fs
                .list_files(limit, query.as_deref(), include_deleted)
                .await?;
            Ok(json!({ "files": files }))
        }
        FsToolsCommand::Search(args) => {
            let mut map = Map::new();
            insert_opt_string(&mut map, "query", args.query);
            insert_opt_u64(&mut map, "limit", args.limit);
            insert_opt_bool(&mut map, "include_deleted", args.include_deleted);
            let payload = payload(args.raw.payload_json, map)?;
            let limit = read_u64(&payload, &["limit"])
                .map(|value| usize::try_from(value).unwrap_or(500))
                .unwrap_or(500)
                .clamp(1, 10_000);
            let query = read_string(&payload, &["query", "q"])
                .ok_or_else(|| anyhow::anyhow!("query is required"))?;
            let include_deleted = read_bool(&payload, &["include_deleted", "includeDeleted"]);
            let files = fs.list_files(limit, Some(&query), include_deleted).await?;
            Ok(json!({ "files": files }))
        }
        FsToolsCommand::Get(args) => {
            let mut map = Map::new();
            insert_opt_string(&mut map, "file_id", args.file_id);
            let payload = payload(args.raw.payload_json, map)?;
            let file_id = read_string(&payload, &["file_id"])
                .ok_or_else(|| anyhow::anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let (record, bytes) = fs.read_all(&file_id).await?;
            Ok(json!({
                "file": record,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }))
        }
        FsToolsCommand::Put(args) => {
            let mut map = Map::new();
            if let Some(kind) = args.kind {
                map.insert("kind".to_string(), Value::String(kind.as_str().to_string()));
            }
            insert_opt_string(&mut map, "session_id", args.session_id);
            insert_opt_string(&mut map, "content_base64", args.content_base64);
            let payload = payload(args.raw.payload_json, map)?;
            let kind = read_string(&payload, &["kind"])
                .and_then(|value| parse_file_kind(&value))
                .unwrap_or(FileKind::Audio);
            let session_id = read_string(&payload, &["session_id"])
                .ok_or_else(|| anyhow::anyhow!("session_id is required"))?;
            let session_id = Uri::parse(&session_id)?;
            let content = read_string(&payload, &["content_base64"])
                .ok_or_else(|| anyhow::anyhow!("content_base64 is required"))?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(content)?;
            let file = fs
                .put_bytes(kind, &bytes, PutFileMetadata { session_id })
                .await?;
            Ok(json!({ "file": file }))
        }
        FsToolsCommand::Delete(args) => {
            let mut map = Map::new();
            insert_opt_string(&mut map, "file_id", args.file_id);
            let payload = payload(args.raw.payload_json, map)?;
            let file_id = read_string(&payload, &["file_id"])
                .ok_or_else(|| anyhow::anyhow!("file_id is required"))?;
            let file_id = Uri::parse(&file_id)?;
            let deleted = fs.soft_delete(&file_id).await?;
            Ok(json!({ "deleted": deleted }))
        }
        FsToolsCommand::Settings => Ok(json!({
            "backend": fs.backend_name(),
            "root_path": fs.root_path(),
            "counts": {
                "total": fs.count_files(true).await?,
                "active": fs.count_files(false).await?,
                "deleted": fs
                    .count_files(true)
                    .await?
                    .saturating_sub(fs.count_files(false).await?),
            }
        })),
    }
}

fn insert_opt_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_opt_u64(map: &mut Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::from(value));
    }
}

fn insert_opt_bool(map: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::Bool(value));
    }
}

fn payload(raw: Option<String>, map: Map<String, Value>) -> Result<Value> {
    if let Some(raw) = raw {
        let value: Value = serde_json::from_str(&raw)
            .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, raw))?;
        if !value.is_object() {
            return Err(anyhow::anyhow!("payload must be a JSON object"));
        }
        return Ok(value);
    }
    Ok(Value::Object(map))
}

fn parse_file_kind(value: &str) -> Option<FileKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "audio" => Some(FileKind::Audio),
        "image" => Some(FileKind::Image),
        "video" => Some(FileKind::Video),
        _ => None,
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
