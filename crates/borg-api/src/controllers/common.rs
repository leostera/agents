use axum::{Json, http::StatusCode, response::IntoResponse};
use borg_core::Uri;
use serde::Serialize;
use tracing::{error, warn};

#[derive(Serialize)]
pub(crate) struct ApiError {
    pub(crate) error: String,
}

#[derive(Serialize)]
pub(crate) struct ApiValidationError {
    pub(crate) error: String,
    pub(crate) details: Vec<ApiFieldError>,
}

#[derive(Serialize)]
pub(crate) struct ApiFieldError {
    pub(crate) field: String,
    pub(crate) message: String,
}

pub(crate) fn api_error(status: StatusCode, error: String) -> axum::response::Response {
    if status.is_server_error() {
        error!(target: "borg_api", status = %status, error = error.as_str(), "api_error");
    } else {
        warn!(target: "borg_api", status = %status, error = error.as_str(), "api_error");
    }
    (status, Json(ApiError { error })).into_response()
}

pub(crate) fn parse_uri_field(field: &str, raw: &str) -> Result<Uri, axum::response::Response> {
    Uri::parse(raw).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiValidationError {
                error: "invalid request".to_string(),
                details: vec![ApiFieldError {
                    field: field.to_string(),
                    message: "must be a valid URI".to_string(),
                }],
            }),
        )
            .into_response()
    })
}
