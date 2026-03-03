use anyhow::{Result, anyhow};
use borg_agent::{BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_db::{BorgDb, CreateClockworkJobInput, UpdateClockworkJobInput};
use serde_json::{Value, json};
use uuid::Uuid;

const MESSAGE_TYPE: &str = "BorgMessage";

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "Clockwork-listJobs",
            "List scheduled clockwork jobs.",
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
            "Clockwork-getJob",
            "Get one clockwork job by id.",
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
            "Clockwork-createJob",
            "Create a clockwork job that delivers a Borg chat message to an actor session.",
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
            "Clockwork-updateJob",
            "Update a clockwork job fields and/or schedule.",
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
            "Clockwork-pauseJob",
            "Pause a clockwork job.",
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
            "Clockwork-resumeJob",
            "Resume a paused clockwork job.",
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
            "Clockwork-cancelJob",
            "Cancel a clockwork job.",
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
            "Clockwork-listRuns",
            "List recorded runs for one clockwork job.",
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

pub fn build_clockwork_toolchain(db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    Toolchain::builder()
        .add_tool(ClockworkTools::list_jobs(db.clone())?)?
        .add_tool(ClockworkTools::get_job(db.clone())?)?
        .add_tool(ClockworkTools::create_job(db.clone())?)?
        .add_tool(ClockworkTools::update_job(db.clone())?)?
        .add_tool(ClockworkTools::pause_job(db.clone())?)?
        .add_tool(ClockworkTools::resume_job(db.clone())?)?
        .add_tool(ClockworkTools::cancel_job(db.clone())?)?
        .add_tool(ClockworkTools::list_runs(db)?)?
        .build()
}

struct ClockworkTools;

impl ClockworkTools {
    fn list_jobs(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Clockwork-listJobs")?;
        Ok(Tool::new(spec, None, move |request| {
            let db = db.clone();
            async move {
                let limit = request
                    .arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(200) as usize;
                let status = request
                    .arguments
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let jobs = db.list_clockwork_jobs(limit, status).await?;
                json_text(json!({ "jobs": jobs }))
            }
        }))
    }

    fn get_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Clockwork-getJob")?;
        Ok(Tool::new(spec, None, move |request| {
            let db = db.clone();
            async move {
                let job_id = req_str(&request.arguments, "job_id")?;
                let job = db.get_clockwork_job(job_id).await?;
                json_text(json!({ "job": job }))
            }
        }))
    }

    fn create_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Clockwork-createJob")?;
        Ok(Tool::new(spec, None, move |request| {
            let db = db.clone();
            async move {
                let kind = req_str(&request.arguments, "kind")?;
                if kind != "once" && kind != "cron" {
                    return Err(anyhow!(
                        "clockwork.validation_failed: kind must be once or cron"
                    ));
                }

                let actor_id = req_str(&request.arguments, "actor_id")?;
                let session_id = req_str(&request.arguments, "session_id")?;
                let message_text = req_str(&request.arguments, "message_text")?;
                if message_text.trim().is_empty() {
                    return Err(anyhow!(
                        "clockwork.validation_failed: message_text cannot be empty"
                    ));
                }

                let schedule_spec = req_obj_clone(&request.arguments, "schedule_spec")?;
                let headers = request
                    .arguments
                    .get("headers")
                    .and_then(Value::as_object)
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or_else(|| json!({}));
                let next_run_at = request
                    .arguments
                    .get("next_run_at")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);

                let job_id = request
                    .arguments
                    .get("job_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("borg:clockwork_job:{}", Uuid::new_v4()));

                db.create_clockwork_job(&CreateClockworkJobInput {
                    job_id: job_id.clone(),
                    kind: kind.to_string(),
                    target_actor_id: actor_id.to_string(),
                    target_session_id: session_id.to_string(),
                    message_type: MESSAGE_TYPE.to_string(),
                    payload: json!({ "text": message_text }),
                    headers,
                    schedule_spec,
                    next_run_at,
                })
                .await?;

                let job = db.get_clockwork_job(&job_id).await?;
                json_text(json!({ "job": job }))
            }
        }))
    }

    fn update_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Clockwork-updateJob")?;
        Ok(Tool::new(spec, None, move |request| {
            let db = db.clone();
            async move {
                let job_id = req_str(&request.arguments, "job_id")?;

                let payload = request
                    .arguments
                    .get("message_text")
                    .and_then(Value::as_str)
                    .map(|text| json!({ "text": text }));

                let headers = request
                    .arguments
                    .get("headers")
                    .and_then(Value::as_object)
                    .cloned()
                    .map(Value::Object);

                let schedule_spec = request
                    .arguments
                    .get("schedule_spec")
                    .and_then(Value::as_object)
                    .cloned()
                    .map(Value::Object);

                let next_run_at = match request.arguments.get("next_run_at") {
                    Some(Value::Null) => Some(None),
                    Some(Value::String(value)) => Some(Some(value.clone())),
                    Some(_) => {
                        return Err(anyhow!(
                            "clockwork.validation_failed: next_run_at must be string or null"
                        ));
                    }
                    None => None,
                };

                let patch = UpdateClockworkJobInput {
                    kind: opt_str(&request.arguments, "kind"),
                    target_actor_id: opt_str(&request.arguments, "actor_id"),
                    target_session_id: opt_str(&request.arguments, "session_id"),
                    message_type: Some(MESSAGE_TYPE.to_string()),
                    payload,
                    headers,
                    schedule_spec,
                    next_run_at,
                };

                db.update_clockwork_job(job_id, &patch).await?;
                let job = db.get_clockwork_job(job_id).await?;
                json_text(json!({ "job": job }))
            }
        }))
    }

    fn pause_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Clockwork-pauseJob", "paused")
    }

    fn resume_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Clockwork-resumeJob", "active")
    }

    fn cancel_job(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        status_tool(db, "Clockwork-cancelJob", "cancelled")
    }

    fn list_runs(db: BorgDb) -> Result<Tool<BorgToolCall, BorgToolResult>> {
        let spec = required_spec("Clockwork-listRuns")?;
        Ok(Tool::new(spec, None, move |request| {
            let db = db.clone();
            async move {
                let job_id = req_str(&request.arguments, "job_id")?;
                let limit = request
                    .arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(200) as usize;
                let runs = db.list_clockwork_job_runs(job_id, limit).await?;
                json_text(json!({ "runs": runs }))
            }
        }))
    }
}

fn status_tool(db: BorgDb, spec_name: &str, status: &'static str) -> Result<Tool<BorgToolCall, BorgToolResult>> {
    let spec = required_spec(spec_name)?;
    Ok(Tool::new(spec, None, move |request| {
        let db = db.clone();
        async move {
            let job_id = req_str(&request.arguments, "job_id")?;
            db.set_clockwork_job_status(job_id, status).await?;
            let job = db.get_clockwork_job(job_id).await?;
            json_text(json!({ "job": job }))
        }
    }))
}

fn req_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("clockwork.validation_failed: missing {}", key))
}

fn opt_str(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn req_obj_clone(arguments: &Value, key: &str) -> Result<Value> {
    let value = arguments
        .get(key)
        .ok_or_else(|| anyhow!("clockwork.validation_failed: missing {}", key))?;
    match value {
        Value::Object(map) => Ok(Value::Object(map.clone())),
        _ => Err(anyhow!(
            "clockwork.validation_failed: {} must be an object",
            key
        )),
    }
}

fn json_text(value: Value) -> Result<ToolResponse<Value>> {
    Ok(ToolResponse {
        content: ToolResultData::Text(serde_json::to_string(&value)?),
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
        .ok_or_else(|| anyhow!("missing clockwork tool spec {}", name))
}
