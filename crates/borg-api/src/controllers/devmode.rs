use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use borg_taskgraph::{CreateTaskInput, TaskGraphStore};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::controllers::common::{api_error, parse_uri_field};

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
    project_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertProjectRequest {
    name: String,
    root_path: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertSpecRequest {
    project_id: String,
    title: String,
    body: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct MaterializeSpecRequest {
    session_uri: String,
    creator_actor_id: String,
    #[serde(default)]
    assignee_actor_id: Option<String>,
    #[serde(default)]
    subtasks: Vec<MaterializeSubtask>,
}

#[derive(Deserialize)]
pub(crate) struct MaterializeSubtask {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
    #[serde(default)]
    assignee_actor_id: Option<String>,
}

pub(crate) struct DevModeController;

impl DevModeController {
    pub(crate) async fn list_projects(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        match state.db.list_devmode_projects(limit).await {
            Ok(projects) => (StatusCode::OK, Json(json!({ "projects": projects }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_project(
        State(state): State<AppState>,
        AxumPath(project_id): AxumPath<String>,
        Json(payload): Json<UpsertProjectRequest>,
    ) -> impl IntoResponse {
        let project_id = match parse_uri_field("project_id", &project_id) {
            Ok(v) => v,
            Err(err) => return err,
        };

        match state
            .db
            .upsert_devmode_project(
                &project_id,
                &payload.name,
                &payload.root_path,
                payload.description.as_deref().unwrap_or(""),
                payload.status.as_deref().unwrap_or("ONGOING"),
            )
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) if err.to_string().contains("project name is required") => {
                api_error(StatusCode::BAD_REQUEST, err.to_string())
            }
            Err(err) if err.to_string().contains("invalid project status") => {
                api_error(StatusCode::BAD_REQUEST, err.to_string())
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn list_specs(
        State(state): State<AppState>,
        Query(query): Query<LimitQuery>,
    ) -> impl IntoResponse {
        let limit = query.limit.unwrap_or(100);
        let project_id = match query.project_id.as_deref() {
            Some(raw) => match parse_uri_field("project_id", raw) {
                Ok(v) => Some(v),
                Err(err) => return err,
            },
            None => None,
        };
        match state
            .db
            .list_devmode_specs(project_id.as_ref(), limit)
            .await
        {
            Ok(specs) => (StatusCode::OK, Json(json!({ "specs": specs }))).into_response(),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn get_spec(
        State(state): State<AppState>,
        AxumPath(spec_id): AxumPath<String>,
    ) -> impl IntoResponse {
        let spec_id = match parse_uri_field("spec_id", &spec_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        match state.db.get_devmode_spec(&spec_id).await {
            Ok(Some(spec)) => (StatusCode::OK, Json(json!({ "spec": spec }))).into_response(),
            Ok(None) => api_error(StatusCode::NOT_FOUND, "spec not found".to_string()),
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn upsert_spec(
        State(state): State<AppState>,
        AxumPath(spec_id): AxumPath<String>,
        Json(payload): Json<UpsertSpecRequest>,
    ) -> impl IntoResponse {
        let spec_id = match parse_uri_field("spec_id", &spec_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let project_id = match parse_uri_field("project_id", &payload.project_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let status = payload.status.as_deref().unwrap_or("DRAFT");
        match state
            .db
            .upsert_devmode_spec(&spec_id, &project_id, &payload.title, &payload.body, status)
            .await
        {
            Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
            Err(err) if err.to_string().contains("project not found") => {
                api_error(StatusCode::NOT_FOUND, err.to_string())
            }
            Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    }

    pub(crate) async fn materialize_spec(
        State(state): State<AppState>,
        AxumPath(spec_id): AxumPath<String>,
        Json(payload): Json<MaterializeSpecRequest>,
    ) -> impl IntoResponse {
        let spec_id = match parse_uri_field("spec_id", &spec_id) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let session_uri = match parse_uri_field("session_uri", &payload.session_uri) {
            Ok(v) => v,
            Err(err) => return err,
        };
        let creator_actor_id = match parse_uri_field("creator_actor_id", &payload.creator_actor_id)
        {
            Ok(v) => v,
            Err(err) => return err,
        };
        let default_assignee = match payload.assignee_actor_id.as_deref() {
            Some(raw) => match parse_uri_field("assignee_actor_id", raw) {
                Ok(v) => v,
                Err(err) => return err,
            },
            None => creator_actor_id.clone(),
        };

        let spec = match state.db.get_devmode_spec(&spec_id).await {
            Ok(Some(spec)) => spec,
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "spec not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let task_store = TaskGraphStore::new(state.db.clone());
        let root_input = CreateTaskInput {
            title: spec.title.clone(),
            description: spec.body.clone(),
            definition_of_done: "Spec fully implemented and ready for review.".to_string(),
            assignee_agent_id: default_assignee.to_string(),
            parent_uri: None,
            blocked_by: Vec::new(),
            references: vec![spec.spec_id.to_string()],
            labels: vec!["source:devmode".to_string(), "kind:spec".to_string()],
        };
        let root = match task_store
            .create_task(session_uri.as_str(), creator_actor_id.as_str(), root_input)
            .await
        {
            Ok(task) => task,
            Err(err) => return api_error(StatusCode::BAD_REQUEST, err.to_string()),
        };

        let subtasks = if payload.subtasks.is_empty() {
            vec![
                MaterializeSubtask {
                    title: format!("Plan: {}", spec.title),
                    description: Some(
                        "Break down implementation details and file touchpoints.".to_string(),
                    ),
                    definition_of_done: Some("Concrete implementation plan exists.".to_string()),
                    assignee_actor_id: None,
                },
                MaterializeSubtask {
                    title: format!("Implement: {}", spec.title),
                    description: Some("Apply code changes required by the spec.".to_string()),
                    definition_of_done: Some(
                        "Code changes compile and satisfy the spec.".to_string(),
                    ),
                    assignee_actor_id: None,
                },
                MaterializeSubtask {
                    title: format!("Validate: {}", spec.title),
                    description: Some("Run tests/checks and capture rollout notes.".to_string()),
                    definition_of_done: Some("Validation evidence is captured.".to_string()),
                    assignee_actor_id: None,
                },
            ]
        } else {
            payload.subtasks
        };

        let mut created_children = Vec::new();
        for subtask in subtasks {
            let assignee = match subtask.assignee_actor_id.as_deref() {
                Some(raw) => match parse_uri_field("subtask.assignee_actor_id", raw) {
                    Ok(v) => v,
                    Err(err) => return err,
                },
                None => default_assignee.clone(),
            };
            let child_input = CreateTaskInput {
                title: subtask.title,
                description: subtask.description.unwrap_or_default(),
                definition_of_done: subtask.definition_of_done.unwrap_or_default(),
                assignee_agent_id: assignee.to_string(),
                parent_uri: Some(root.uri.clone()),
                blocked_by: Vec::new(),
                references: vec![spec.spec_id.to_string()],
                labels: vec![
                    "source:devmode".to_string(),
                    "kind:spec-subtask".to_string(),
                ],
            };
            match task_store
                .create_task(session_uri.as_str(), creator_actor_id.as_str(), child_input)
                .await
            {
                Ok(child) => created_children.push(child),
                Err(err) => return api_error(StatusCode::BAD_REQUEST, err.to_string()),
            }
        }

        if let Err(err) = state
            .db
            .mark_devmode_spec_taskgraphed(&spec.spec_id, &root.uri)
            .await
        {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }

        let updated_spec = match state.db.get_devmode_spec(&spec.spec_id).await {
            Ok(Some(spec)) => spec,
            Ok(None) => return api_error(StatusCode::NOT_FOUND, "spec not found".to_string()),
            Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        (
            StatusCode::OK,
            Json(json!({
                "spec": updated_spec,
                "root_task": root,
                "subtasks": created_children
            })),
        )
            .into_response()
    }
}
