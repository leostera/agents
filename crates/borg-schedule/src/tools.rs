use anyhow::{Result, anyhow};
use borg_agent::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
};
use borg_db::{BorgDb, CreateScheduleJobInput, UpdateScheduleJobInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

const MESSAGE_TYPE: &str = "BorgMessage";

#[derive(Debug, Clone, Deserialize)]
struct ListJobsArgs {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JobIdArgs {
    job_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ListRunsArgs {
    job_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ScheduleSpecDto {
    Once { run_at: String },
    Cron { cron: String },
}

#[derive(Debug, Clone, Deserialize)]
struct CreateJobArgs {
    #[serde(default)]
    job_id: Option<String>,
    kind: String,
    actor_id: String,
    session_id: String,
    message_text: String,
    schedule_spec: ScheduleSpecDto,
    #[serde(default)]
    next_run_at: Option<String>,
    #[serde(default)]
    headers: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateJobArgs {
    job_id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    actor_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    message_text: Option<String>,
    #[serde(default)]
    schedule_spec: Option<ScheduleSpecDto>,
    #[serde(default)]
    next_run_at: Option<Option<String>>,
    #[serde(default)]
    headers: Option<BTreeMap<String, String>>,
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "Schedule-listJobs",
            "List scheduled schedule jobs.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number" },
                    "status": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-getJob",
            "Get one schedule job by id.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-createJob",
            "Create a schedule job that delivers a Borg chat message to an actor session.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["once", "cron"] },
                    "actor_id": { "type": "string" },
                    "session_id": { "type": "string" },
                    "message_text": { "type": "string" },
                    "schedule_spec": { "type": "object", "additionalProperties": true },
                    "next_run_at": { "type": "string" },
                    "headers": { "type": "object", "additionalProperties": true }
                },
                "required": ["kind", "actor_id", "session_id", "message_text", "schedule_spec"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-updateJob",
            "Update a schedule job fields and/or schedule.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["once", "cron"] },
                    "actor_id": { "type": "string" },
                    "session_id": { "type": "string" },
                    "message_text": { "type": "string" },
                    "schedule_spec": { "type": "object", "additionalProperties": true },
                    "next_run_at": {},
                    "headers": { "type": "object", "additionalProperties": true }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-pauseJob",
            "Pause a schedule job.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-resumeJob",
            "Resume a paused schedule job.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-cancelJob",
            "Cancel a schedule job.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Schedule-listRuns",
            "List recorded runs for one schedule job.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" },
                    "limit": { "type": "number" }
                },
                "required": ["job_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub fn build_schedule_toolchain(db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    Toolchain::builder()
        .add_tool(ScheduleTools::list_jobs(db.clone())?)?
        .add_tool(ScheduleTools::get_job(db.clone())?)?
        .add_tool(ScheduleTools::create_job(db.clone())?)?
        .add_tool(ScheduleTools::update_job(db.clone())?)?
        .add_tool(ScheduleTools::pause_job(db.clone())?)?
        .add_tool(ScheduleTools::resume_job(db.clone())?)?
        .add_tool(ScheduleTools::cancel_job(db.clone())?)?
        .add_tool(ScheduleTools::list_runs(db)?)?
        .build()
}

struct ScheduleTools;

impl ScheduleTools {
    fn list_jobs(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Schedule-listJobs")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ListJobsArgs>| {
                let db = db.clone();
                async move {
                    let limit = request.arguments.limit.unwrap_or(200);
                    let status = request
                        .arguments
                        .status
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let jobs = db.list_schedule_jobs(limit, status).await?;
                    json_text(&json!({ "jobs": jobs }))
                }
            },
        ))
    }

    fn get_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Schedule-getJob")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<JobIdArgs>| {
                let db = db.clone();
                async move {
                    let job_id = require_non_empty(&request.arguments.job_id, "job_id")?;
                    let job = db.get_schedule_job(&job_id).await?;
                    json_text(&json!({ "job": job }))
                }
            },
        ))
    }

    fn create_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Schedule-createJob")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<CreateJobArgs>| {
                let db = db.clone();
                async move {
                    let kind = require_non_empty(&request.arguments.kind, "kind")?;
                    if kind != "once" && kind != "cron" {
                        return Err(anyhow!(
                            "schedule.validation_failed: kind must be once or cron"
                        ));
                    }

                    let actor_id = require_non_empty(&request.arguments.actor_id, "actor_id")?;
                    let session_id =
                        require_non_empty(&request.arguments.session_id, "session_id")?;
                    let message_text =
                        require_non_empty(&request.arguments.message_text, "message_text")?;
                    if message_text.trim().is_empty() {
                        return Err(anyhow!(
                            "schedule.validation_failed: message_text cannot be empty"
                        ));
                    }

                    let schedule_spec = serde_json::to_value(&request.arguments.schedule_spec)?;
                    let headers =
                        serde_json::to_value(request.arguments.headers.unwrap_or_default())?;
                    let next_run_at = option_non_empty(request.arguments.next_run_at);

                    let job_id = request
                        .arguments
                        .job_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("borg:schedule_job:{}", Uuid::new_v4()));

                    db.create_schedule_job(&CreateScheduleJobInput {
                        job_id: job_id.clone(),
                        kind,
                        target_actor_id: actor_id,
                        target_session_id: session_id,
                        message_type: MESSAGE_TYPE.to_string(),
                        payload: json!({ "text": message_text }),
                        headers,
                        schedule_spec,
                        next_run_at,
                    })
                    .await?;

                    let job = db.get_schedule_job(&job_id).await?;
                    json_text(&json!({ "job": job }))
                }
            },
        ))
    }

    fn update_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Schedule-updateJob")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<UpdateJobArgs>| {
                let db = db.clone();
                async move {
                    let job_id = require_non_empty(&request.arguments.job_id, "job_id")?;

                    let payload = request
                        .arguments
                        .message_text
                        .as_deref()
                        .map(|text| json!({ "text": text }));

                    let headers = request
                        .arguments
                        .headers
                        .map(serde_json::to_value)
                        .transpose()?;
                    let schedule_spec = request
                        .arguments
                        .schedule_spec
                        .map(|value| serde_json::to_value(value))
                        .transpose()?;
                    let next_run_at = request.arguments.next_run_at;

                    let patch = UpdateScheduleJobInput {
                        kind: option_non_empty(request.arguments.kind),
                        target_actor_id: option_non_empty(request.arguments.actor_id),
                        target_session_id: option_non_empty(request.arguments.session_id),
                        message_type: Some(MESSAGE_TYPE.to_string()),
                        payload,
                        headers,
                        schedule_spec,
                        next_run_at,
                    };

                    db.update_schedule_job(&job_id, &patch).await?;
                    let job = db.get_schedule_job(&job_id).await?;
                    json_text(&json!({ "job": job }))
                }
            },
        ))
    }

    fn pause_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Schedule-pauseJob", "paused")
    }

    fn resume_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Schedule-resumeJob", "active")
    }

    fn cancel_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Schedule-cancelJob", "cancelled")
    }

    fn list_runs(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Schedule-listRuns")?;
        Ok(Tool::new_transcoded(
            spec,
            None,
            move |request: borg_agent::ToolRequest<ListRunsArgs>| {
                let db = db.clone();
                async move {
                    let job_id = require_non_empty(&request.arguments.job_id, "job_id")?;
                    let limit = request.arguments.limit.unwrap_or(200);
                    let runs = db.list_schedule_job_runs(&job_id, limit).await?;
                    json_text(&json!({ "runs": runs }))
                }
            },
        ))
    }
}

fn status_tool(
    db: BorgDb,
    spec_name: &str,
    status: &'static str,
) -> Result<Tool<BorgToolCall, BorgToolResult>> {
    let spec = required_spec(spec_name)?;
    Ok(Tool::new_transcoded(
        spec,
        None,
        move |request: borg_agent::ToolRequest<JobIdArgs>| {
            let db = db.clone();
            async move {
                let job_id = require_non_empty(&request.arguments.job_id, "job_id")?;
                db.set_schedule_job_status(&job_id, status).await?;
                let job = db.get_schedule_job(&job_id).await?;
                json_text(&json!({ "job": job }))
            }
        },
    ))
}

fn json_text<T: Serialize>(value: &T) -> Result<ToolResponse<()>> {
    Ok(ToolResponse {
        content: ToolResultData::Text(serde_json::to_string(value)?),
    })
}

fn require_non_empty(value: &str, key: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("schedule.validation_failed: missing {}", key));
    }
    Ok(trimmed.to_string())
}

fn option_non_empty(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
        .ok_or_else(|| anyhow!("missing schedule tool spec {}", name))
}
