use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use include_dir::{Dir, include_dir};
use tokio::fs;
use tracing::{debug, info, warn};

static DASHBOARD_DIST: Dir<'_> = include_dir!("$BORG_DASHBOARD_DIST_ABS");
const INDEX_HTML_PATH: &str = "index.html";
const ASSETS_PREFIX: &str = "assets/";

#[derive(Debug, Clone)]
pub struct DashboardService {
    assets_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct DashboardSyncSummary {
    pub files_written: usize,
}

impl DashboardService {
    pub fn new(assets_dir: PathBuf) -> Self {
        Self { assets_dir }
    }

    pub async fn sync_static_media_assets(&self) -> Result<DashboardSyncSummary> {
        fs::create_dir_all(&self.assets_dir).await?;

        let mut files_written = 0usize;
        for file in DASHBOARD_DIST.files() {
            let relative = normalize_file_path(file.path());
            if !Self::is_static_media_asset(relative.as_str()) {
                continue;
            }
            let target_path = self.assets_dir.join(relative.as_str());
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            let contents = file.contents();
            let should_write = match fs::read(&target_path).await {
                Ok(existing) => existing != contents,
                Err(_) => true,
            };
            if should_write {
                fs::write(&target_path, contents).await?;
                files_written += 1;
            }
        }

        Ok(DashboardSyncSummary { files_written })
    }

    pub fn log_sync_result(&self, summary: DashboardSyncSummary) {
        info!(
            target: "borg_api",
            assets_dir = %self.assets_dir.display(),
            files_written = summary.files_written,
            "dashboard static media assets synced"
        );
    }

    pub fn dashboard_index_response(&self) -> Response {
        match DASHBOARD_DIST.get_file(INDEX_HTML_PATH) {
            Some(file) => {
                let mut response = file.contents().to_vec().into_response();
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static("text/html; charset=utf-8"),
                );
                response
            }
            None => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "dashboard index asset missing in binary",
            )
                .into_response(),
        }
    }

    pub async fn asset_response(&self, requested_path: &str) -> Response {
        let Some(safe_path) = sanitize_asset_path(requested_path) else {
            return (StatusCode::BAD_REQUEST, "invalid asset path").into_response();
        };
        let relative = format!("{ASSETS_PREFIX}{safe_path}");
        let disk_path = self.assets_dir.join(relative.as_str());
        if let Ok(bytes) = fs::read(&disk_path).await {
            debug!(
                target: "borg_api",
                path = %disk_path.display(),
                "serving dashboard asset from disk"
            );
            return bytes_response(bytes, relative.as_str());
        }

        if let Some(file) = DASHBOARD_DIST.get_file(relative.as_str()) {
            warn!(
                target: "borg_api",
                requested = requested_path,
                "dashboard asset missing on disk; serving embedded fallback"
            );
            return bytes_response(file.contents().to_vec(), relative.as_str());
        }

        (StatusCode::NOT_FOUND, "asset not found").into_response()
    }

    fn is_static_media_asset(path: &str) -> bool {
        match Path::new(path).extension().and_then(|value| value.to_str()) {
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "avif" | "ico" | "woff" | "woff2"
                | "ttf" | "otf",
            ) => true,
            _ => false,
        }
    }
}

fn bytes_response(bytes: Vec<u8>, path: &str) -> Response {
    let mut response = bytes.into_response();
    if let Some(content_type) = guess_content_type(path) {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
    }
    response
}

fn normalize_file_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_asset_path(path: &str) -> Option<String> {
    let candidate = Path::new(path);
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return None;
    }

    let normalized = candidate
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn guess_content_type(path: &str) -> Option<header::HeaderValue> {
    let value = match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => return None,
    };
    header::HeaderValue::from_str(value).ok()
}
