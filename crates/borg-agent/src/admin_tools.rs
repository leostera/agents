use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use borg_core::{ActorId, WorkspaceId};
use borg_db::BorgDb;

use crate::{Tool, ToolCall, ToolResponse, ToolResult, ToolResultData, ToolSpec, Toolchain};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoAmIArgs {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActorArgs {
    #[serde(default)]
    pub actor_id: Option<ActorId>,
    pub name: String,
    pub model: String,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateActorArgs {
    pub actor_id: ActorId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableActorArgs {
    pub actor_id: ActorId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListActorsArgs {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorAdminResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<borg_db::ActorRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actors: Option<Vec<borg_db::ActorRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<ActorId>,
}

pub fn default_actor_admin_tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "Actors-listActors",
            "List registered actors.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Actors-whoAmI",
            "Return the current runtime actor identity.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Actors-createActor",
            "Create a new actor.",
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
            "Actors-updateActor",
            "Update an existing actor.",
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
            "Actors-disableActor",
            "Disable an existing actor.",
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

pub fn build_actor_admin_toolchain<TToolCall, TToolResult>(
    db: BorgDb,
    current_actor_id: ActorId,
) -> Result<Toolchain<TToolCall, TToolResult>>
where
    TToolCall: ToolCall,
    TToolResult: ToolResult,
{
    let whoami_actor_id = current_actor_id.clone();
    let workspace_id = WorkspaceId::from_id("default");

    let db_list = db.clone();
    let db_create = db.clone();
    let db_update = db.clone();
    let db_disable = db;

    Toolchain::builder()
        .add_tool(Tool::new_transcoded(
            required_spec("Actors-listActors")?,
            None,
            move |request: crate::ToolRequest<ListActorsArgs>| {
                let db = db_list.clone();
                async move {
                    let limit = request.arguments.limit.unwrap_or(100);
                    let agents = db.list_actors(limit).await?;
                    Ok(ToolResponse {
                        output: ToolResultData::Ok(ActorAdminResult {
                            actor: None,
                            actors: Some(agents),
                            actor_id: None,
                        }),
                    })
                }
            },
        ))?
        .add_tool(Tool::new_transcoded(
            required_spec("Actors-whoAmI")?,
            None,
            move |_request: crate::ToolRequest<WhoAmIArgs>| {
                let actor_id = whoami_actor_id.clone();
                async move {
                    Ok(ToolResponse {
                        output: ToolResultData::Ok(ActorAdminResult {
                            actor: None,
                            actors: None,
                            actor_id: Some(actor_id),
                        }),
                    })
                }
            },
        ))?
        .add_tool({
            let db = db_create;
            let workspace_id = workspace_id.clone();
            Tool::new_transcoded(
                required_spec("Actors-createActor").unwrap(),
                None,
                move |request: crate::ToolRequest<CreateActorArgs>| {
                    let db = db.clone();
                    let workspace_id = workspace_id.clone();
                    async move {
                        let actor_id = request
                            .arguments
                            .actor_id
                            .clone()
                            .unwrap_or_else(|| ActorId::new());
                        let name = require_non_empty(&request.arguments.name, "name")?;
                        let model = require_non_empty(&request.arguments.model, "model")?;
                        let system_prompt =
                            require_non_empty(&request.arguments.system_prompt, "system_prompt")?;
                        db.upsert_actor(
                            &actor_id,
                            &workspace_id,
                            &name,
                            &system_prompt,
                            "",
                            "RUNNING",
                        )
                        .await?;
                        db.set_actor_model(&actor_id, &model).await?;

                        let agent = db
                            .get_actor(&actor_id)
                            .await?
                            .ok_or_else(|| anyhow!("actor.not_found"))?;
                        Ok(ToolResponse {
                            output: ToolResultData::Ok(ActorAdminResult {
                                actor: Some(agent),
                                actors: None,
                                actor_id: None,
                            }),
                        })
                    }
                },
            )
        })?
        .add_tool({
            let db = db_update;
            let workspace_id = workspace_id.clone();
            Tool::new_transcoded(
                required_spec("Actors-updateActor").unwrap(),
                None,
                move |request: crate::ToolRequest<UpdateActorArgs>| {
                    let db = db.clone();
                    let workspace_id = workspace_id.clone();
                    async move {
                        let actor_id = request.arguments.actor_id.clone();
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
                        db.upsert_actor(
                            &actor_id,
                            &workspace_id,
                            name.as_str(),
                            system_prompt.as_str(),
                            "",
                            "RUNNING",
                        )
                        .await?;
                        db.set_actor_model(&actor_id, model.as_str()).await?;

                        let agent = db
                            .get_actor(&actor_id)
                            .await?
                            .ok_or_else(|| anyhow!("actor.not_found"))?;
                        Ok(ToolResponse {
                            output: ToolResultData::Ok(ActorAdminResult {
                                actor: Some(agent),
                                actors: None,
                                actor_id: None,
                            }),
                        })
                    }
                },
            )
        })?
        .add_tool({
            let db = db_disable;
            let workspace_id = workspace_id;
            Tool::new_transcoded(
                required_spec("Actors-disableActor").unwrap(),
                None,
                move |request: crate::ToolRequest<DisableActorArgs>| {
                    let db = db.clone();
                    let workspace_id = workspace_id.clone();
                    async move {
                        let actor_id = request.arguments.actor_id.clone();
                        let existing = db
                            .get_actor(&actor_id)
                            .await?
                            .ok_or_else(|| anyhow!("actor.not_found"))?;
                        db.upsert_actor(
                            &actor_id,
                            &workspace_id,
                            existing.name.as_str(),
                            existing.system_prompt.as_str(),
                            "",
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
                        Ok(ToolResponse {
                            output: ToolResultData::Ok(ActorAdminResult {
                                actor: Some(agent),
                                actors: None,
                                actor_id: None,
                            }),
                        })
                    }
                },
            )
        })?
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
    default_actor_admin_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("missing actor admin tool spec {}", name))
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
