use axum::extract::{OriginalUri, Path as AxumPath, State};
use axum::http::{StatusCode, Uri};
use axum::response::IntoResponse;
use tracing::debug;

use crate::AppState;

pub(crate) struct DashboardController;

impl DashboardController {
    pub(crate) async fn index(State(state): State<AppState>) -> impl IntoResponse {
        debug!(target: "borg_api", "dashboard index endpoint called");
        state.dashboard.dashboard_index_response()
    }

    pub(crate) async fn asset(
        State(state): State<AppState>,
        AxumPath(path): AxumPath<String>,
    ) -> impl IntoResponse {
        state.dashboard.asset_response(path.as_str()).await
    }

    pub(crate) async fn spa_fallback(
        State(state): State<AppState>,
        OriginalUri(uri): OriginalUri,
    ) -> impl IntoResponse {
        if Self::is_reserved_runtime_path(&uri) || Self::looks_like_static_file(&uri) {
            return StatusCode::NOT_FOUND.into_response();
        }
        state.dashboard.dashboard_index_response()
    }

    fn is_reserved_runtime_path(uri: &Uri) -> bool {
        let path = uri.path();
        path == "/api"
            || path.starts_with("/api/")
            || path == "/ports"
            || path.starts_with("/ports/")
            || path == "/health"
            || path == "/oauth"
            || path.starts_with("/oauth/")
    }

    fn looks_like_static_file(uri: &Uri) -> bool {
        let path = uri.path();
        path.rsplit('/')
            .next()
            .map(|segment| segment.contains('.'))
            .unwrap_or(false)
    }
}
