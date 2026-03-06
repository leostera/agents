use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use borg_core::{Uri, uri};
use borg_db::BorgDb;

use crate::{
    BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain,
};

#[derive(Debug, Clone, Deserialize)]
struct ListAgentsArgs {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct WhoAmIArgs {}

#[derive(Debug, Clone, Deserialize)]
struct CreateAgentArgs {
    #[serde(default)]
    actor_id: Option<String>,
    name: String,
    model: String,
    system_prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateAgentArgs {
    actor_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DisableAgentArgs {
    actor_id: String,
}

pub fn default_agent_admin_tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "Agents-listAgents",
            "List registered agent specs.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Agents-whoAmI",
            "Return the current runtime identity (agent + session).",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Agents-createAgent",
            "Create a new agent spec.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "name": { "type": "string" },
                    "model": { "type": "string" },
                    "system_prompt": { "type": "string" }
                },
                "required": ["name", "model", "system_prompt"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Agents-updateAgent",
            "Update an existing agent spec.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" },
                    "name": { "type": "string" },
                    "model": { "type": "string" },
                    "system_prompt": { "type": "string" }
                },
                "required": ["actor_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Agents-disableAgent",
            "Disable an existing agent spec.",
            json!({
                "type": "object",
                "properties": {
                    "actor_id": { "type": "string", "format": "uri" }
                },
                "required": ["actor_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub fn build_agent_admin_toolchain(
    db: BorgDb,
    current_session_id: Uri,
    current_actor_id: Uri,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let whoami_actor_id = current_actor_id.to_string();
    let whoami_session_id = current_session_id.to_string();

    let db_list = db.clone();
    let db_create = db.clone();
    let db_update = db.clone();
    let db_disable = db;

    Toolchain::builder()
        .add_tool(Tool::new_transcoded(
            required_spec("Agents-listAgents")?,
            None,
            move |request: crate::ToolRequest<ListAgentsArgs>| {
                let db = db_list.clone();
                async move {
                    let limit = request.arguments.limit.unwrap_or(100);
                    let agents = db.list_actors(limit).await?;
                    json_text(&json!({ "agents": agents }))
                }
            },
        ))?
        .add_tool(Tool::new_transcoded(
            required_spec("Agents-whoAmI")?,
            None,
            move |_request: crate::ToolRequest<WhoAmIArgs>| {
                let actor_id = whoami_actor_id.clone();
                let session_id = whoami_session_id.clone();
                async move {
                    json_text(&json!({
                        "actor_id": actor_id,
                        "session_id": session_id
                    }))
                }
            },
        ))?
        .add_tool(Tool::new_transcoded(
            required_spec("Agents-createAgent")?,
            None,
            move |request: crate::ToolRequest<CreateAgentArgs>| {
                let db = db_create.clone();
                async move {
                    let actor_id = request
                        .arguments
                        .actor_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(Uri::parse)
                        .transpose()?
                        .unwrap_or_else(|| uri!("borg", "actor", &Uuid::now_v7().to_string()));
                    let name = require_non_empty(&request.arguments.name, "name")?;
                    let model = require_non_empty(&request.arguments.model, "model")?;
                    let system_prompt =
                        require_non_empty(&request.arguments.system_prompt, "system_prompt")?;
                    db.upsert_actor(&actor_id, &name, &system_prompt, "RUNNING")
                        .await?;
                    db.set_actor_model(&actor_id, &model).await?;

                    let agent = db
                        .get_actor(&actor_id)
                        .await?
                        .ok_or_else(|| anyhow!("actor.not_found"))?;
                    json_text(&json!({ "agent": agent }))
                }
            },
        ))?
        .add_tool(Tool::new_transcoded(
            required_spec("Agents-updateAgent")?,
            None,
            move |request: crate::ToolRequest<UpdateAgentArgs>| {
                let db = db_update.clone();
                async move {
                    let actor_id =
                        Uri::parse(&require_non_empty(&request.arguments.actor_id, "actor_id")?)?;
                    let existing = db
                        .get_actor(&actor_id)
                        .await?
                        .ok_or_else(|| anyhow!("actor.not_found"))?;

                    let name = option_non_empty(request.arguments.name)
                        .unwrap_or_else(|| existing.name.clone());
                    let model = option_non_empty(request.arguments.model)
                        .or(existing.model.clone())
                        .ok_or_else(|| anyhow!("actor.model_not_set"))?;
                    let system_prompt = option_non_empty(request.arguments.system_prompt)
                        .unwrap_or(existing.system_prompt);
                    db.upsert_actor(&actor_id, name.as_str(), system_prompt.as_str(), "RUNNING")
                        .await?;
                    db.set_actor_model(&actor_id, model.as_str()).await?;

                    let agent = db
                        .get_actor(&actor_id)
                        .await?
                        .ok_or_else(|| anyhow!("actor.not_found"))?;
                    json_text(&json!({ "agent": agent }))
                }
            },
        ))?
        .add_tool(Tool::new_transcoded(
            required_spec("Agents-disableAgent")?,
            None,
            move |request: crate::ToolRequest<DisableAgentArgs>| {
                let db = db_disable.clone();
                async move {
                    let actor_id =
                        Uri::parse(&require_non_empty(&request.arguments.actor_id, "actor_id")?)?;
                    let existing = db
                        .get_actor(&actor_id)
                        .await?
                        .ok_or_else(|| anyhow!("actor.not_found"))?;
                    db.upsert_actor(
                        &actor_id,
                        existing.name.as_str(),
                        existing.system_prompt.as_str(),
                        "STOPPED",
                    )
                    .await?;
                    if let Some(model) = existing.model {
                        db.set_actor_model(&actor_id, model.as_str()).await?;
                    }
                    let agent = db
                        .get_actor(&actor_id)
                        .await?
                        .ok_or_else(|| anyhow!("actor.not_found"))?;
                    json_text(&json!({ "agent": agent }))
                }
            },
        ))?
        .build()
}

fn tool_spec(name: &str, description: &str, parameters: Value) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

fn required_spec(name: &str) -> Result<ToolSpec> {
    default_agent_admin_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("missing agent admin tool spec {}", name))
}

fn json_text<T: Serialize>(value: &T) -> Result<ToolResponse<()>> {
    Ok(ToolResponse {
        content: ToolResultData::Text(serde_json::to_string(value)?),
    })
}

fn require_non_empty(value: &str, key: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("validation_failed: missing {}", key));
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
