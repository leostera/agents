use anyhow::{Result, anyhow};
use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
};
use borg_db::BorgDb;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::model::TaskStatus;
use crate::store::{CreateTaskInput, ListParams, SplitSubtaskInput, TaskGraphStore, TaskPatch};

#[derive(Debug, Clone, Deserialize)]
struct CreateTaskArgs {
    actor_id: String,
    creator_actor_id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
    assignee_actor_id: String,
    #[serde(default)]
    labels: Option<Vec<String>>,
    #[serde(default)]
    parent_uri: Option<String>,
    #[serde(default)]
    blocked_by: Option<Vec<String>>,
    #[serde(default)]
    references: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct GetTaskArgs {
    uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ListTasksArgs {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskPatchArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateTaskFieldsArgs {
    actor_id: String,
    uri: String,
    patch: TaskPatchArgs,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriUriArgs {
    actor_id: String,
    uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReassignAssigneeArgs {
    actor_id: String,
    uri: String,
    assignee_actor_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriUriLabelsArgs {
    actor_id: String,
    uri: String,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SetTaskParentArgs {
    actor_id: String,
    uri: String,
    parent_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UriListArgs {
    uri: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriUriBlockedByArgs {
    actor_id: String,
    uri: String,
    blocked_by: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriUriDuplicateOfArgs {
    actor_id: String,
    uri: String,
    duplicate_of: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriUriReferenceArgs {
    actor_id: String,
    uri: String,
    reference: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SetTaskStatusArgs {
    actor_id: String,
    uri: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RequestReviewChangesArgs {
    actor_id: String,
    uri: String,
    note: String,
    #[serde(default)]
    return_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SplitTaskIntoSubtasksArgs {
    actor_id: String,
    creator_actor_id: String,
    uri: String,
    subtasks: Vec<SplitSubtaskArgs>,
}

#[derive(Debug, Clone, Deserialize)]
struct SplitSubtaskArgs {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
    assignee_actor_id: String,
    #[serde(default)]
    labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AddCommentArgs {
    actor_id: String,
    task_uri: String,
    body: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskUriListArgs {
    task_uri: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActorUriLimitArgs {
    actor_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "TaskGraph-createTask",
            "Create a new task and allocate fresh assignee/reviewer actors.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "creator_actor_id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "definition_of_done": { "type": "string" },
                    "assignee_actor_id": { "type": "string" },
                    "labels": { "type": "array", "items": { "type": "string" } },
                    "parent_uri": { "type": "string", "format": "uri" },
                    "blocked_by": { "type": "array", "items": { "type": "string", "format": "uri" } },
                    "references": { "type": "array", "items": { "type": "string", "format": "uri" } }
                },
                "required": ["actor_id", "creator_actor_id", "title", "assignee_actor_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-getTask",
            "Get one task by URI.",
            json!({
                "type": "object",
                "properties": {
                    "uri": { "type": "string", "format": "uri" }
                },
                "required": ["uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-listTasks",
            "List top-level tasks with cursor pagination.",
            json!({
                "type": "object",
                "properties": {
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": [],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-updateTaskFields",
            "Patch title/description/definition_of_done for a task.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "patch": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "description": { "type": "string" },
                            "definition_of_done": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["actor_id", "uri", "patch"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-reassignAssignee",
            "Reviewer-only reassignment that allocates a fresh assignee actor.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "assignee_actor_id": { "type": "string" }
                },
                "required": ["actor_id", "uri", "assignee_actor_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-addTaskLabels",
            "Add labels to a task.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "labels": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
                },
                "required": ["actor_id", "uri", "labels"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-removeTaskLabels",
            "Remove labels from a task.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "labels": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
                },
                "required": ["actor_id", "uri", "labels"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-setTaskParent",
            "Set task parent.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "parent_uri": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "parent_uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-clearTaskParent",
            "Clear task parent.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-listTaskChildren",
            "List children for a parent task.",
            json!({
                "type": "object",
                "properties": {
                    "uri": { "type": "string", "format": "uri" },
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-addTaskBlockedBy",
            "Add blocked_by dependency edge.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "blocked_by": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "blocked_by"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-removeTaskBlockedBy",
            "Remove blocked_by dependency edge.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "blocked_by": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "blocked_by"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-setTaskDuplicateOf",
            "Set duplicate_of and discard duplicate subtree.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "duplicate_of": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "duplicate_of"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-clearTaskDuplicateOf",
            "Clear duplicate_of relationship.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-listDuplicatedBy",
            "List tasks whose duplicate_of points to this task.",
            json!({
                "type": "object",
                "properties": {
                    "uri": { "type": "string", "format": "uri" },
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-addTaskReference",
            "Add reference edge.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "reference": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "reference"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-removeTaskReference",
            "Remove reference edge.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "reference": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri", "reference"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-setTaskStatus",
            "Set task status for assignee/reviewer transitions.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "status": { "type": "string", "enum": ["pending", "doing", "review", "done", "discarded"] }
                },
                "required": ["actor_id", "uri", "status"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-submitReview",
            "Submit work for review; transitions to review status.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-approveReview",
            "Approve review and mark done.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id", "uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-requestReviewChanges",
            "Request review changes and return status to pending/doing.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "uri": { "type": "string", "format": "uri" },
                    "return_to": { "type": "string", "enum": ["pending", "doing"] },
                    "note": { "type": "string" }
                },
                "required": ["actor_id", "uri", "note"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-splitTaskIntoSubtasks",
            "Split a task into explicit subtasks.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "creator_actor_id": { "type": "string" },
                    "uri": { "type": "string", "format": "uri" },
                    "subtasks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "definition_of_done": { "type": "string" },
                                "assignee_actor_id": { "type": "string" },
                                "labels": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["title", "assignee_actor_id"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["actor_id", "creator_actor_id", "uri", "subtasks"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-addComment",
            "Add an append-only comment. Any actor may comment.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "task_uri": { "type": "string", "format": "uri" },
                    "body": { "type": "string" }
                },
                "required": ["actor_id", "task_uri", "body"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-listComments",
            "List task comments with cursor pagination.",
            json!({
                "type": "object",
                "properties": {
                    "task_uri": { "type": "string", "format": "uri" },
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["task_uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-listEvents",
            "List task audit events with cursor pagination.",
            json!({
                "type": "object",
                "properties": {
                    "task_uri": { "type": "string", "format": "uri" },
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["task_uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-nextTask",
            "Return next queue-eligible tasks for an actor.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["actor_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "TaskGraph-reconcileInProgress",
            "Return currently eligible in-progress tasks for an actor.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["actor_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub fn build_taskgraph_toolchain(db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let store = TaskGraphStore::new(db);

    Toolchain::builder()
        .add_tool(TaskGraphTools::create_task(store.clone())?)?
        .add_tool(TaskGraphTools::get_task(store.clone())?)?
        .add_tool(TaskGraphTools::list_tasks(store.clone())?)?
        .add_tool(TaskGraphTools::update_task_fields(store.clone())?)?
        .add_tool(TaskGraphTools::reassign_assignee(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_labels(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_labels(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_parent(store.clone())?)?
        .add_tool(TaskGraphTools::clear_task_parent(store.clone())?)?
        .add_tool(TaskGraphTools::list_task_children(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_blocked_by(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_blocked_by(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_duplicate_of(store.clone())?)?
        .add_tool(TaskGraphTools::clear_task_duplicate_of(store.clone())?)?
        .add_tool(TaskGraphTools::list_duplicated_by(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_reference(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_reference(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_status(store.clone())?)?
        .add_tool(TaskGraphTools::submit_review(store.clone())?)?
        .add_tool(TaskGraphTools::approve_review(store.clone())?)?
        .add_tool(TaskGraphTools::request_review_changes(store.clone())?)?
        .add_tool(TaskGraphTools::split_task_into_subtasks(store.clone())?)?
        .add_tool(TaskGraphTools::add_comment(store.clone())?)?
        .add_tool(TaskGraphTools::list_comments(store.clone())?)?
        .add_tool(TaskGraphTools::list_events(store.clone())?)?
        .add_tool(TaskGraphTools::next_task(store.clone())?)?
        .add_tool(TaskGraphTools::reconcile_in_progress(store)?)?
        .build()
}

pub fn build_taskgraph_worker_toolchain(
    db: BorgDb,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let store = TaskGraphStore::new(db);

    Toolchain::builder()
        .add_tool(TaskGraphTools::get_task(store.clone())?)?
        .add_tool(TaskGraphTools::list_tasks(store.clone())?)?
        .add_tool(TaskGraphTools::update_task_fields(store.clone())?)?
        .add_tool(TaskGraphTools::reassign_assignee(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_labels(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_labels(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_parent(store.clone())?)?
        .add_tool(TaskGraphTools::clear_task_parent(store.clone())?)?
        .add_tool(TaskGraphTools::list_task_children(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_blocked_by(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_blocked_by(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_duplicate_of(store.clone())?)?
        .add_tool(TaskGraphTools::clear_task_duplicate_of(store.clone())?)?
        .add_tool(TaskGraphTools::list_duplicated_by(store.clone())?)?
        .add_tool(TaskGraphTools::add_task_reference(store.clone())?)?
        .add_tool(TaskGraphTools::remove_task_reference(store.clone())?)?
        .add_tool(TaskGraphTools::set_task_status(store.clone())?)?
        .add_tool(TaskGraphTools::submit_review(store.clone())?)?
        .add_tool(TaskGraphTools::approve_review(store.clone())?)?
        .add_tool(TaskGraphTools::request_review_changes(store.clone())?)?
        .add_tool(TaskGraphTools::add_comment(store.clone())?)?
        .add_tool(TaskGraphTools::list_comments(store.clone())?)?
        .add_tool(TaskGraphTools::list_events(store.clone())?)?
        .add_tool(TaskGraphTools::next_task(store.clone())?)?
        .add_tool(TaskGraphTools::reconcile_in_progress(store)?)?
        .build()
}

struct TaskGraphTools;

impl TaskGraphTools {
    fn create_task(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-createTask")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<CreateTaskArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = request.arguments.actor_id.trim().to_string();
                    let creator_actor_id = request.arguments.creator_actor_id.trim().to_string();
                    let title = request.arguments.title.trim().to_string();
                    let assignee_actor_id = request.arguments.assignee_actor_id.trim().to_string();
                    if actor_id.is_empty()
                        || creator_actor_id.is_empty()
                        || title.is_empty()
                        || assignee_actor_id.is_empty()
                    {
                        return Err(anyhow!("task.validation_failed: missing required fields"));
                    }
                    let input = CreateTaskInput {
                        title,
                        description: request
                            .arguments
                            .description
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        definition_of_done: request
                            .arguments
                            .definition_of_done
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        assignee_actor_id,
                        parent_uri: option_non_empty(request.arguments.parent_uri),
                        blocked_by: normalize_strs(request.arguments.blocked_by),
                        references: normalize_strs(request.arguments.references),
                        labels: normalize_strs(request.arguments.labels),
                    };

                    let task = store
                        .create_task(&actor_id, &creator_actor_id, input)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn get_task(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-getTask")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<GetTaskArgs>| {
                let store = store.clone();
                async move {
                    let uri = request.arguments.uri.trim().to_string();
                    if uri.is_empty() {
                        return Err(anyhow!("task.validation_failed: missing uri"));
                    }
                    let task = store.get_task(&uri).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn list_tasks(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-listTasks")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ListTasksArgs>| {
                let store = store.clone();
                async move {
                    let params = list_params(request.arguments.cursor, request.arguments.limit);
                    let (tasks, next_cursor) = store.list_tasks(params).await?;
                    json_text(json!({ "tasks": tasks, "next_cursor": next_cursor }))
                }
            },
        ))
    }

    fn update_task_fields(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-updateTaskFields")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<UpdateTaskFieldsArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = request.arguments.actor_id.trim().to_string();
                    let uri = request.arguments.uri.trim().to_string();
                    if actor_id.is_empty() || uri.is_empty() {
                        return Err(anyhow!("task.validation_failed: missing required fields"));
                    }
                    let patch = TaskPatch {
                        title: option_non_empty(request.arguments.patch.title),
                        description: option_non_empty(request.arguments.patch.description),
                        definition_of_done: option_non_empty(
                            request.arguments.patch.definition_of_done,
                        ),
                    };
                    let task = store.update_task_fields(&actor_id, &uri, patch).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn reassign_assignee(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-reassignAssignee")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ReassignAssigneeArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let assignee_actor_id =
                        required_str(request.arguments.assignee_actor_id, "assignee_actor_id")?;
                    let task = store
                        .reassign_assignee(&actor_id, &uri, &assignee_actor_id)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn add_task_labels(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-addTaskLabels")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriLabelsArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let labels = normalize_strs(Some(request.arguments.labels));
                    let task = store.add_task_labels(&actor_id, &uri, &labels).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn remove_task_labels(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-removeTaskLabels")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriLabelsArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let labels = normalize_strs(Some(request.arguments.labels));
                    let task = store.remove_task_labels(&actor_id, &uri, &labels).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn set_task_parent(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-setTaskParent")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<SetTaskParentArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let parent_uri = required_str(request.arguments.parent_uri, "parent_uri")?;
                    let (child, parent) =
                        store.set_task_parent(&actor_id, &uri, &parent_uri).await?;
                    json_text(json!({ "child": child, "parent": parent }))
                }
            },
        ))
    }

    fn clear_task_parent(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-clearTaskParent")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let task = store.clear_task_parent(&actor_id, &uri).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn list_task_children(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-listTaskChildren")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<UriListArgs>| {
                let store = store.clone();
                async move {
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let params = list_params(request.arguments.cursor, request.arguments.limit);
                    let (children, next_cursor) = store.list_task_children(&uri, params).await?;
                    json_text(json!({ "children": children, "next_cursor": next_cursor }))
                }
            },
        ))
    }

    fn add_task_blocked_by(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-addTaskBlockedBy")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriBlockedByArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let blocked_by = required_str(request.arguments.blocked_by, "blocked_by")?;
                    let task = store
                        .add_task_blocked_by(&actor_id, &uri, &blocked_by)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn remove_task_blocked_by(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-removeTaskBlockedBy")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriBlockedByArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let blocked_by = required_str(request.arguments.blocked_by, "blocked_by")?;
                    let task = store
                        .remove_task_blocked_by(&actor_id, &uri, &blocked_by)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn set_task_duplicate_of(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-setTaskDuplicateOf")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriDuplicateOfArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let duplicate_of =
                        required_str(request.arguments.duplicate_of, "duplicate_of")?;
                    let task = store
                        .set_task_duplicate_of(&actor_id, &uri, &duplicate_of)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn clear_task_duplicate_of(
        store: TaskGraphStore,
    ) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-clearTaskDuplicateOf")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let task = store.clear_task_duplicate_of(&actor_id, &uri).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn list_duplicated_by(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-listDuplicatedBy")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<UriListArgs>| {
                let store = store.clone();
                async move {
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let params = list_params(request.arguments.cursor, request.arguments.limit);
                    let (tasks, next_cursor) = store.list_duplicated_by(&uri, params).await?;
                    json_text(json!({ "tasks": tasks, "next_cursor": next_cursor }))
                }
            },
        ))
    }

    fn add_task_reference(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-addTaskReference")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriReferenceArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let reference = required_str(request.arguments.reference, "reference")?;
                    let task = store
                        .add_task_reference(&actor_id, &uri, &reference)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn remove_task_reference(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-removeTaskReference")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriReferenceArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let reference = required_str(request.arguments.reference, "reference")?;
                    let task = store
                        .remove_task_reference(&actor_id, &uri, &reference)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn set_task_status(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-setTaskStatus")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<SetTaskStatusArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let status = parse_status(&required_str(request.arguments.status, "status")?)?;
                    let task = store.set_task_status(&actor_id, &uri, status).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn submit_review(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-submitReview")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let task = store.submit_review(&actor_id, &uri).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn approve_review(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-approveReview")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriUriArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let task = store.approve_review(&actor_id, &uri).await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn request_review_changes(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-requestReviewChanges")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<RequestReviewChangesArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let note = required_str(request.arguments.note, "note")?;
                    let return_to =
                        parse_status(request.arguments.return_to.as_deref().unwrap_or("doing"))?;
                    let task = store
                        .request_review_changes(&actor_id, &uri, return_to, &note)
                        .await?;
                    json_text(json!({ "task": task }))
                }
            },
        ))
    }

    fn split_task_into_subtasks(
        store: TaskGraphStore,
    ) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-splitTaskIntoSubtasks")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<SplitTaskIntoSubtasksArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let creator_actor_id =
                        required_str(request.arguments.creator_actor_id, "creator_actor_id")?;
                    let uri = required_str(request.arguments.uri, "uri")?;
                    let subtasks = request
                        .arguments
                        .subtasks
                        .into_iter()
                        .map(|subtask| {
                            Ok(SplitSubtaskInput {
                                title: required_str(subtask.title, "subtask title")?,
                                description: subtask
                                    .description
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string(),
                                definition_of_done: subtask
                                    .definition_of_done
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string(),
                                assignee_actor_id: required_str(
                                    subtask.assignee_actor_id,
                                    "subtask assignee_actor_id",
                                )?,
                                labels: normalize_strs(subtask.labels),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let (parent, created) = store
                        .split_task_into_subtasks(&actor_id, &creator_actor_id, &uri, subtasks)
                        .await?;
                    json_text(json!({ "parent": parent, "created": created }))
                }
            },
        ))
    }

    fn add_comment(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-addComment")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<AddCommentArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let task_uri = required_str(request.arguments.task_uri, "task_uri")?;
                    let body = required_str(request.arguments.body, "body")?;
                    let comment = store.add_comment(&actor_id, &task_uri, &body).await?;
                    json_text(json!({ "comment": comment }))
                }
            },
        ))
    }

    fn list_comments(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-listComments")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<TaskUriListArgs>| {
                let store = store.clone();
                async move {
                    let task_uri = required_str(request.arguments.task_uri, "task_uri")?;
                    let params = list_params(request.arguments.cursor, request.arguments.limit);
                    let (comments, next_cursor) = store.list_comments(&task_uri, params).await?;
                    json_text(json!({ "comments": comments, "next_cursor": next_cursor }))
                }
            },
        ))
    }

    fn list_events(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-listEvents")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<TaskUriListArgs>| {
                let store = store.clone();
                async move {
                    let task_uri = required_str(request.arguments.task_uri, "task_uri")?;
                    let params = list_params(request.arguments.cursor, request.arguments.limit);
                    let (events, next_cursor) = store.list_events(&task_uri, params).await?;
                    json_text(json!({ "events": events, "next_cursor": next_cursor }))
                }
            },
        ))
    }

    fn next_task(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-nextTask")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriLimitArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let limit = request.arguments.limit.unwrap_or(1);
                    let tasks = store.next_task(&actor_id, limit).await?;
                    json_text(json!({ "tasks": tasks }))
                }
            },
        ))
    }

    fn reconcile_in_progress(store: TaskGraphStore) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("TaskGraph-reconcileInProgress")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ActorUriLimitArgs>| {
                let store = store.clone();
                async move {
                    let actor_id = required_str(request.arguments.actor_id, "actor_id")?;
                    let limit = request.arguments.limit.unwrap_or(25);
                    let tasks = store.reconcile_in_progress(&actor_id, limit).await?;
                    json_text(json!({ "tasks": tasks }))
                }
            },
        ))
    }
}

fn list_params(cursor: Option<String>, limit: Option<usize>) -> ListParams {
    ListParams {
        cursor,
        limit: limit.unwrap_or(50),
    }
}

fn parse_status(value: &str) -> Result<TaskStatus> {
    TaskStatus::parse(value).ok_or_else(|| anyhow!("task.validation_failed: invalid status"))
}

fn option_non_empty(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_strs(values: Option<Vec<String>>) -> Vec<String> {
    values
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn required_str(value: String, key: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("task.validation_failed: missing {}", key));
    }
    Ok(trimmed.to_string())
}

fn json_text(value: Value) -> Result<ToolResponse<Value>> {
    Ok(ToolResponse {
        output: ToolResultData::Ok(value),
    })
}

fn tool_spec(name: &str, description: &str, parameters: Value) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

fn required_spec(name: &str) -> Result<ToolSpec> {
    default_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("missing taskgraph tool spec {}", name))
}
