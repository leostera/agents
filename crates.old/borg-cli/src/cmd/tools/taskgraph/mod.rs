use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Map, Value, json};

use super::decode_tool_response;

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum TaskGraphCommand {
    #[command(about = "List TaskGraph commands")]
    List,
    #[command(about = "Create a new task and allocate assignee/reviewer actors")]
    CreateTask(CreateTaskArgs),
    #[command(about = "Get one task by URI")]
    GetTask(GetTaskArgs),
    #[command(about = "List top-level tasks")]
    ListTasks(ListTasksArgs),
    #[command(about = "Delete a task (marks status as discarded)")]
    Delete(ActorAndUriArgs),
    #[command(about = "Patch title/description/definition_of_done for a task")]
    UpdateTaskFields(UpdateTaskFieldsArgs),
    #[command(about = "Reviewer-only reassignment to a new assignee actor")]
    ReassignAssignee(ReassignAssigneeArgs),
    #[command(about = "Add labels to a task")]
    AddTaskLabels(TaskLabelsArgs),
    #[command(about = "Remove labels from a task")]
    RemoveTaskLabels(TaskLabelsArgs),
    #[command(about = "Set task parent")]
    SetTaskParent(SetTaskParentArgs),
    #[command(about = "Clear task parent")]
    ClearTaskParent(ActorAndUriArgs),
    #[command(about = "List children for a parent task")]
    ListTaskChildren(ListByUriArgs),
    #[command(about = "Add blocked_by dependency edge")]
    AddTaskBlockedBy(TaskBlockedByArgs),
    #[command(about = "Remove blocked_by dependency edge")]
    RemoveTaskBlockedBy(TaskBlockedByArgs),
    #[command(about = "Set duplicate_of relationship")]
    SetTaskDuplicateOf(TaskDuplicateOfArgs),
    #[command(about = "Clear duplicate_of relationship")]
    ClearTaskDuplicateOf(ActorAndUriArgs),
    #[command(about = "List tasks duplicated by this task")]
    ListDuplicatedBy(ListByUriArgs),
    #[command(about = "Add reference edge")]
    AddTaskReference(TaskReferenceArgs),
    #[command(about = "Remove reference edge")]
    RemoveTaskReference(TaskReferenceArgs),
    #[command(about = "Set task status")]
    SetTaskStatus(SetTaskStatusArgs),
    #[command(about = "Submit work for review")]
    SubmitReview(ActorAndUriArgs),
    #[command(about = "Approve review and mark done")]
    ApproveReview(ActorAndUriArgs),
    #[command(about = "Request review changes")]
    RequestReviewChanges(RequestReviewChangesArgs),
    #[command(about = "Split task into explicit subtasks")]
    SplitTaskIntoSubtasks(SplitTaskIntoSubtasksArgs),
    #[command(about = "Add an append-only comment")]
    AddComment(AddCommentArgs),
    #[command(about = "List task comments")]
    ListComments(ListByTaskUriArgs),
    #[command(about = "List task audit events")]
    ListEvents(ListByTaskUriArgs),
    #[command(about = "Return next queue-eligible tasks for a actor")]
    NextTask(ListByActorArgs),
    #[command(about = "Return in-progress tasks eligible for a actor")]
    ReconcileInProgress(ListByActorArgs),
}

#[derive(Args, Debug)]
pub struct RawPayloadArg {
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

#[derive(Args, Debug)]
pub struct ActorAndUriArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct CreateTaskArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub creator_actor_id: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub assignee_actor_id: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub definition_of_done: Option<String>,
    #[arg(long = "label")]
    pub labels: Vec<String>,
    #[arg(long)]
    pub parent_uri: Option<String>,
    #[arg(long = "blocked-by")]
    pub blocked_by: Vec<String>,
    #[arg(long = "reference")]
    pub references: Vec<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct GetTaskArgs {
    #[arg(long)]
    pub uri: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ListTasksArgs {
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct UpdateTaskFieldsArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub definition_of_done: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ReassignAssigneeArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub assignee_actor_id: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct TaskLabelsArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long = "label")]
    pub labels: Vec<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct SetTaskParentArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub parent_uri: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ListByUriArgs {
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct TaskBlockedByArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub blocked_by: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct TaskDuplicateOfArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub duplicate_of: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct TaskReferenceArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub reference: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TaskStatusArg {
    Pending,
    Doing,
    Review,
    Done,
    Discarded,
}

impl TaskStatusArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Doing => "doing",
            Self::Review => "review",
            Self::Done => "done",
            Self::Discarded => "discarded",
        }
    }
}

#[derive(Args, Debug)]
pub struct SetTaskStatusArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub status: Option<TaskStatusArg>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ReturnToArg {
    Pending,
    Doing,
}

impl ReturnToArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Doing => "doing",
        }
    }
}

#[derive(Args, Debug)]
pub struct RequestReviewChangesArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long)]
    pub return_to: Option<ReturnToArg>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct SplitTaskIntoSubtasksArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub creator_actor_id: Option<String>,
    #[arg(long)]
    pub uri: Option<String>,
    #[arg(long, value_name = "JSON_ARRAY", help = "Subtasks JSON array")]
    pub subtasks_json: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct AddCommentArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub task_uri: Option<String>,
    #[arg(long)]
    pub body: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ListByTaskUriArgs {
    #[arg(long)]
    pub task_uri: Option<String>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ListByActorArgs {
    #[arg(long)]
    pub actor_id: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

pub fn command_names() -> Vec<&'static str> {
    vec![
        "create-task",
        "get-task",
        "list-tasks",
        "delete",
        "update-task-fields",
        "reassign-assignee",
        "add-task-labels",
        "remove-task-labels",
        "set-task-parent",
        "clear-task-parent",
        "list-task-children",
        "add-task-blocked-by",
        "remove-task-blocked-by",
        "set-task-duplicate-of",
        "clear-task-duplicate-of",
        "list-duplicated-by",
        "add-task-reference",
        "remove-task-reference",
        "set-task-status",
        "submit-review",
        "approve-review",
        "request-review-changes",
        "split-task-into-subtasks",
        "add-comment",
        "list-comments",
        "list-events",
        "next-task",
        "reconcile-in-progress",
    ]
}

pub async fn run(app: &BorgCliApp, cmd: TaskGraphCommand) -> Result<Value> {
    match cmd {
        TaskGraphCommand::List => {
            Ok(json!({"ok": true, "namespace": "taskgraph", "commands": command_names()}))
        }
        TaskGraphCommand::CreateTask(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "creator_actor_id", args.creator_actor_id);
            insert_req(&mut map, "title", args.title);
            insert_req(&mut map, "assignee_actor_id", args.assignee_actor_id);
            insert_opt(&mut map, "description", args.description);
            insert_opt(&mut map, "definition_of_done", args.definition_of_done);
            insert_vec(&mut map, "labels", args.labels);
            insert_opt(&mut map, "parent_uri", args.parent_uri);
            insert_vec(&mut map, "blocked_by", args.blocked_by);
            insert_vec(&mut map, "references", args.references);
            execute(
                app,
                "create-task",
                "TaskGraph-createTask",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::GetTask(args) => {
            execute_map(
                app,
                "get-task",
                "TaskGraph-getTask",
                args.raw.payload_json,
                [req("uri", args.uri)],
            )
            .await
        }
        TaskGraphCommand::ListTasks(args) => {
            execute_list_tasks(app, "list-tasks", "TaskGraph-listTasks", args).await
        }
        TaskGraphCommand::Delete(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "uri", args.uri);
            map.insert("status".to_string(), Value::String("discarded".to_string()));
            execute(
                app,
                "delete",
                "TaskGraph-setTaskStatus",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::UpdateTaskFields(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "uri", args.uri);
            let mut patch = Map::new();
            insert_opt(&mut patch, "title", args.title);
            insert_opt(&mut patch, "description", args.description);
            insert_opt(&mut patch, "definition_of_done", args.definition_of_done);
            map.insert("patch".to_string(), Value::Object(patch));
            execute(
                app,
                "update-task-fields",
                "TaskGraph-updateTaskFields",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::ReassignAssignee(args) => {
            execute_map(
                app,
                "reassign-assignee",
                "TaskGraph-reassignAssignee",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("assignee_actor_id", args.assignee_actor_id),
                ],
            )
            .await
        }
        TaskGraphCommand::AddTaskLabels(args) => {
            execute_labels(app, "add-task-labels", "TaskGraph-addTaskLabels", args).await
        }
        TaskGraphCommand::RemoveTaskLabels(args) => {
            execute_labels(
                app,
                "remove-task-labels",
                "TaskGraph-removeTaskLabels",
                args,
            )
            .await
        }
        TaskGraphCommand::SetTaskParent(args) => {
            execute_map(
                app,
                "set-task-parent",
                "TaskGraph-setTaskParent",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("parent_uri", args.parent_uri),
                ],
            )
            .await
        }
        TaskGraphCommand::ClearTaskParent(args) => {
            execute_map(
                app,
                "clear-task-parent",
                "TaskGraph-clearTaskParent",
                args.raw.payload_json,
                [req("actor_id", args.actor_id), req("uri", args.uri)],
            )
            .await
        }
        TaskGraphCommand::ListTaskChildren(args) => {
            execute_list_by_uri(
                app,
                "list-task-children",
                "TaskGraph-listTaskChildren",
                args,
            )
            .await
        }
        TaskGraphCommand::AddTaskBlockedBy(args) => {
            execute_map(
                app,
                "add-task-blocked-by",
                "TaskGraph-addTaskBlockedBy",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("blocked_by", args.blocked_by),
                ],
            )
            .await
        }
        TaskGraphCommand::RemoveTaskBlockedBy(args) => {
            execute_map(
                app,
                "remove-task-blocked-by",
                "TaskGraph-removeTaskBlockedBy",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("blocked_by", args.blocked_by),
                ],
            )
            .await
        }
        TaskGraphCommand::SetTaskDuplicateOf(args) => {
            execute_map(
                app,
                "set-task-duplicate-of",
                "TaskGraph-setTaskDuplicateOf",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("duplicate_of", args.duplicate_of),
                ],
            )
            .await
        }
        TaskGraphCommand::ClearTaskDuplicateOf(args) => {
            execute_map(
                app,
                "clear-task-duplicate-of",
                "TaskGraph-clearTaskDuplicateOf",
                args.raw.payload_json,
                [req("actor_id", args.actor_id), req("uri", args.uri)],
            )
            .await
        }
        TaskGraphCommand::ListDuplicatedBy(args) => {
            execute_list_by_uri(
                app,
                "list-duplicated-by",
                "TaskGraph-listDuplicatedBy",
                args,
            )
            .await
        }
        TaskGraphCommand::AddTaskReference(args) => {
            execute_map(
                app,
                "add-task-reference",
                "TaskGraph-addTaskReference",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("reference", args.reference),
                ],
            )
            .await
        }
        TaskGraphCommand::RemoveTaskReference(args) => {
            execute_map(
                app,
                "remove-task-reference",
                "TaskGraph-removeTaskReference",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("uri", args.uri),
                    req("reference", args.reference),
                ],
            )
            .await
        }
        TaskGraphCommand::SetTaskStatus(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "uri", args.uri);
            if let Some(status) = args.status {
                map.insert(
                    "status".to_string(),
                    Value::String(status.as_str().to_string()),
                );
            }
            execute(
                app,
                "set-task-status",
                "TaskGraph-setTaskStatus",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::SubmitReview(args) => {
            execute_map(
                app,
                "submit-review",
                "TaskGraph-submitReview",
                args.raw.payload_json,
                [req("actor_id", args.actor_id), req("uri", args.uri)],
            )
            .await
        }
        TaskGraphCommand::ApproveReview(args) => {
            execute_map(
                app,
                "approve-review",
                "TaskGraph-approveReview",
                args.raw.payload_json,
                [req("actor_id", args.actor_id), req("uri", args.uri)],
            )
            .await
        }
        TaskGraphCommand::RequestReviewChanges(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "uri", args.uri);
            insert_req(&mut map, "note", args.note);
            if let Some(rt) = args.return_to {
                map.insert(
                    "return_to".to_string(),
                    Value::String(rt.as_str().to_string()),
                );
            }
            execute(
                app,
                "request-review-changes",
                "TaskGraph-requestReviewChanges",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::SplitTaskIntoSubtasks(args) => {
            let mut map = Map::new();
            insert_req(&mut map, "actor_id", args.actor_id);
            insert_req(&mut map, "creator_actor_id", args.creator_actor_id);
            insert_req(&mut map, "uri", args.uri);
            if let Some(subtasks) = args.subtasks_json {
                map.insert("subtasks".to_string(), serde_json::from_str(&subtasks)?);
            }
            execute(
                app,
                "split-task-into-subtasks",
                "TaskGraph-splitTaskIntoSubtasks",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        TaskGraphCommand::AddComment(args) => {
            execute_map(
                app,
                "add-comment",
                "TaskGraph-addComment",
                args.raw.payload_json,
                [
                    req("actor_id", args.actor_id),
                    req("task_uri", args.task_uri),
                    req("body", args.body),
                ],
            )
            .await
        }
        TaskGraphCommand::ListComments(args) => {
            execute_list_by_task_uri(app, "list-comments", "TaskGraph-listComments", args).await
        }
        TaskGraphCommand::ListEvents(args) => {
            execute_list_by_task_uri(app, "list-events", "TaskGraph-listEvents", args).await
        }
        TaskGraphCommand::NextTask(args) => {
            execute_list_by_actor(app, "next-task", "TaskGraph-nextTask", args).await
        }
        TaskGraphCommand::ReconcileInProgress(args) => {
            execute_list_by_actor(
                app,
                "reconcile-in-progress",
                "TaskGraph-reconcileInProgress",
                args,
            )
            .await
        }
    }
}

fn req(key: &str, value: Option<String>) -> (String, Option<String>) {
    (key.to_string(), value)
}

fn insert_req(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_opt(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_vec(map: &mut Map<String, Value>, key: &str, values: Vec<String>) {
    if !values.is_empty() {
        map.insert(
            key.to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
}

fn payload(raw: Option<String>, map: Map<String, Value>) -> Result<String> {
    if let Some(raw) = raw {
        return Ok(raw);
    }
    Ok(Value::Object(map).to_string())
}

async fn execute_map<const N: usize>(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    raw: Option<String>,
    fields: [(String, Option<String>); N],
) -> Result<Value> {
    let mut map = Map::new();
    for (key, value) in fields {
        if let Some(value) = value {
            map.insert(key, Value::String(value));
        }
    }
    execute(app, command, tool_name, payload(raw, map)?).await
}

async fn execute_labels(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    args: TaskLabelsArgs,
) -> Result<Value> {
    let mut map = Map::new();
    insert_req(&mut map, "actor_id", args.actor_id);
    insert_req(&mut map, "uri", args.uri);
    insert_vec(&mut map, "labels", args.labels);
    execute(
        app,
        command,
        tool_name,
        payload(args.raw.payload_json, map)?,
    )
    .await
}

async fn execute_list_by_uri(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    args: ListByUriArgs,
) -> Result<Value> {
    let mut map = Map::new();
    insert_req(&mut map, "uri", args.uri);
    insert_opt(&mut map, "cursor", args.cursor);
    if let Some(limit) = args.limit {
        map.insert("limit".to_string(), Value::from(limit));
    }
    execute(
        app,
        command,
        tool_name,
        payload(args.raw.payload_json, map)?,
    )
    .await
}

async fn execute_list_by_task_uri(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    args: ListByTaskUriArgs,
) -> Result<Value> {
    let mut map = Map::new();
    insert_req(&mut map, "task_uri", args.task_uri);
    insert_opt(&mut map, "cursor", args.cursor);
    if let Some(limit) = args.limit {
        map.insert("limit".to_string(), Value::from(limit));
    }
    execute(
        app,
        command,
        tool_name,
        payload(args.raw.payload_json, map)?,
    )
    .await
}

async fn execute_list_by_actor(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    args: ListByActorArgs,
) -> Result<Value> {
    let mut map = Map::new();
    insert_req(&mut map, "actor_id", args.actor_id);
    if let Some(limit) = args.limit {
        map.insert("limit".to_string(), Value::from(limit));
    }
    execute(
        app,
        command,
        tool_name,
        payload(args.raw.payload_json, map)?,
    )
    .await
}

async fn execute_list_tasks(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    args: ListTasksArgs,
) -> Result<Value> {
    let mut map = Map::new();
    insert_opt(&mut map, "cursor", args.cursor);
    if let Some(limit) = args.limit {
        map.insert("limit".to_string(), Value::from(limit));
    }
    execute(
        app,
        command,
        tool_name,
        payload(args.raw.payload_json, map)?,
    )
    .await
}

async fn execute(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    payload: String,
) -> Result<Value> {
    let arguments: Value = serde_json::from_str(&payload)
        .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, payload))?;
    let db = app.open_config_db().await?;
    db.migrate().await?;
    let toolchain = borg_taskgraph::build_taskgraph_toolchain(db)?;
    let response = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: format!("cli-taskgraph-{}", command),
            tool_name: tool_name.to_string(),
            arguments: arguments.into(),
        })
        .await?;

    decode_tool_response(response)
}
