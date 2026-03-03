use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::controllers::common::api_error;

pub(crate) struct FsController;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ListFilesQuery {
    limit: Option<usize>,
    q: Option<String>,
    include_deleted: Option<bool>,
}

impl FsController {
    pub(crate) async fn list_files(
        State(state): State<AppState>,
        Query(query): Query<ListFilesQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(500).clamp(1, 10_000);
        match state
            .db
            .list_files(
                limit,
                query.q.as_deref(),
                query.include_deleted.unwrap_or(false),
            )
            .await
        {
            Ok(files) => (StatusCode::OK, Json(json!({ "files": files }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn settings(State(state): State<AppState>) -> impl IntoResponse {
        let total = match state.db.count_files(true).await {
            Ok(count) => count,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        let active = match state.db.count_files(false).await {
            Ok(count) => count,
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        (
            StatusCode::OK,
            Json(json!({
                "backend": state.files.backend_name(),
                "root_path": state.files.root_path(),
                "counts": {
                    "total": total,
                    "active": active,
                    "deleted": total.saturating_sub(active),
                }
            })),
        )
            .into_response()
    }
}
