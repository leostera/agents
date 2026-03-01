use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn default_admin_tool_specs() -> Vec<ToolSpec> {
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
                    "agent_id": { "type": "string", "format": "uri" },
                    "name": { "type": "string" },
                    "default_provider_id": { "type": "string" },
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
                    "agent_id": { "type": "string", "format": "uri" },
                    "name": { "type": "string" },
                    "default_provider_id": { "type": "string" },
                    "model": { "type": "string" },
                    "system_prompt": { "type": "string" }
                },
                "required": ["agent_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Agents-disableAgent",
            "Disable an existing agent spec.",
            json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "format": "uri" }
                },
                "required": ["agent_id"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Ports-listPorts",
            "List configured ports.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Ports-createPort",
            "Create a port configuration.",
            json!({
                "type": "object",
                "properties": {
                    "port_uri": { "type": "string", "format": "uri" },
                    "provider": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "allows_guests": { "type": "boolean" },
                    "default_agent_id": { "type": "string", "format": "uri" },
                    "settings": { "type": "object" }
                },
                "required": ["port_uri", "provider"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Ports-updatePort",
            "Update a port configuration.",
            json!({
                "type": "object",
                "properties": {
                    "port_uri": { "type": "string", "format": "uri" },
                    "provider": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "allows_guests": { "type": "boolean" },
                    "default_agent_id": { "type": "string", "format": "uri" },
                    "settings": { "type": "object" }
                },
                "required": ["port_uri"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Providers-listProviders",
            "List configured providers.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Providers-createProvider",
            "Create a provider configuration.",
            json!({
                "type": "object",
                "properties": {
                    "provider": { "type": "string" },
                    "api_key": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "default_text_model": { "type": "string" },
                    "default_audio_model": { "type": "string" }
                },
                "required": ["provider", "api_key"],
                "additionalProperties": false
            }),
        ),
        tool_spec(
            "Providers-updateProvider",
            "Update a provider configuration.",
            json!({
                "type": "object",
                "properties": {
                    "provider": { "type": "string" },
                    "api_key": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "default_text_model": { "type": "string" },
                    "default_audio_model": { "type": "string" }
                },
                "required": ["provider"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub fn build_admin_toolchain(
    db: BorgDb,
    current_session_id: Uri,
    current_agent_id: Uri,
) -> Result<Toolchain> {
    let whoami_agent_id = current_agent_id.to_string();
    let whoami_session_id = current_session_id.to_string();
    let db_agents_list = db.clone();
    let db_agents_create = db.clone();
    let db_agents_update = db.clone();
    let db_agents_disable = db.clone();
    let db_ports_list = db.clone();
    let db_ports_create = db.clone();
    let db_ports_update = db.clone();
    let db_providers_list = db.clone();
    let db_providers_create = db.clone();
    let db_providers_update = db;

    Toolchain::builder()
        .add_tool(Tool::new(
            required_spec("Agents-listAgents")?,
            None,
            move |request| {
                let db = db_agents_list.clone();
                async move {
                    let limit = request
                        .arguments
                        .get("limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize;
                    let agents = db.list_agent_specs(limit).await?;
                    json_text(json!({ "agents": agents }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Agents-whoAmI")?,
            None,
            move |_request| {
                let agent_id = whoami_agent_id.clone();
                let session_id = whoami_session_id.clone();
                async move {
                    json_text(json!({
                        "agent_id": agent_id,
                        "session_id": session_id
                    }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Agents-createAgent")?,
            None,
            move |request| {
                let db = db_agents_create.clone();
                async move {
                    let agent_id = request
                        .arguments
                        .get("agent_id")
                        .and_then(Value::as_str)
                        .map(Uri::parse)
                        .transpose()?
                        .unwrap_or_else(|| uri!("borg", "agent", &Uuid::now_v7().to_string()));
                    let name = req_str(&request.arguments, "name")?;
                    let model = req_str(&request.arguments, "model")?;
                    let system_prompt = req_str(&request.arguments, "system_prompt")?;
                    let default_provider_id =
                        opt_trimmed_str(&request.arguments, "default_provider_id");

                    db.upsert_agent_spec(
                        &agent_id,
                        name,
                        default_provider_id.as_deref(),
                        model,
                        system_prompt,
                    )
                    .await?;

                    let agent = db
                        .get_agent_spec(&agent_id)
                        .await?
                        .ok_or_else(|| anyhow!("agent.not_found"))?;
                    json_text(json!({ "agent": agent }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Agents-updateAgent")?,
            None,
            move |request| {
                let db = db_agents_update.clone();
                async move {
                    let agent_id = Uri::parse(req_str(&request.arguments, "agent_id")?)?;
                    let existing = db
                        .get_agent_spec(&agent_id)
                        .await?
                        .ok_or_else(|| anyhow!("agent.not_found"))?;

                    let name = opt_trimmed_str(&request.arguments, "name")
                        .unwrap_or_else(|| existing.name.clone());
                    let model = opt_trimmed_str(&request.arguments, "model")
                        .unwrap_or_else(|| existing.model.clone());
                    let system_prompt = request
                        .arguments
                        .get("system_prompt")
                        .and_then(Value::as_str)
                        .unwrap_or(existing.system_prompt.as_str())
                        .to_string();
                    let default_provider_id = request
                        .arguments
                        .get("default_provider_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .map(ToOwned::to_owned)
                        .or(existing.default_provider_id.clone())
                        .filter(|value| !value.is_empty());

                    db.upsert_agent_spec(
                        &agent_id,
                        name.as_str(),
                        default_provider_id.as_deref(),
                        model.as_str(),
                        system_prompt.as_str(),
                    )
                    .await?;

                    let agent = db
                        .get_agent_spec(&agent_id)
                        .await?
                        .ok_or_else(|| anyhow!("agent.not_found"))?;
                    json_text(json!({ "agent": agent }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Agents-disableAgent")?,
            None,
            move |request| {
                let db = db_agents_disable.clone();
                async move {
                    let agent_id = Uri::parse(req_str(&request.arguments, "agent_id")?)?;
                    let updated = db.set_agent_spec_enabled(&agent_id, false).await?;
                    if updated == 0 {
                        return Err(anyhow!("agent.not_found"));
                    }
                    let agent = db
                        .get_agent_spec(&agent_id)
                        .await?
                        .ok_or_else(|| anyhow!("agent.not_found"))?;
                    json_text(json!({ "agent": agent }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Ports-listPorts")?,
            None,
            move |request| {
                let db = db_ports_list.clone();
                async move {
                    let limit = request
                        .arguments
                        .get("limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(200) as usize;
                    let ports = db.list_ports(limit).await?;
                    json_text(json!({ "ports": ports }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Ports-createPort")?,
            None,
            move |request| {
                let db = db_ports_create.clone();
                async move {
                    let port_uri = Uri::parse(req_str(&request.arguments, "port_uri")?)?;
                    let port_name = port_name_from_uri(&port_uri)?;
                    if db.get_port(&port_name).await?.is_some() {
                        return Err(anyhow!("port.already_exists"));
                    }
                    let provider = req_str(&request.arguments, "provider")?;
                    let enabled = request
                        .arguments
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let allows_guests = request
                        .arguments
                        .get("allows_guests")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let default_agent_id = request
                        .arguments
                        .get("default_agent_id")
                        .and_then(Value::as_str)
                        .map(Uri::parse)
                        .transpose()?;
                    let settings = request
                        .arguments
                        .get("settings")
                        .cloned()
                        .unwrap_or_else(|| json!({}));

                    db.upsert_port(
                        &port_name,
                        provider,
                        enabled,
                        allows_guests,
                        default_agent_id.as_ref(),
                        &settings,
                    )
                    .await?;

                    let port = db
                        .get_port(&port_name)
                        .await?
                        .ok_or_else(|| anyhow!("port.not_found"))?;
                    json_text(json!({ "port": port }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Ports-updatePort")?,
            None,
            move |request| {
                let db = db_ports_update.clone();
                async move {
                    let port_uri = Uri::parse(req_str(&request.arguments, "port_uri")?)?;
                    let port_name = port_name_from_uri(&port_uri)?;
                    let existing = db
                        .get_port(&port_name)
                        .await?
                        .ok_or_else(|| anyhow!("port.not_found"))?;

                    let provider = opt_trimmed_str(&request.arguments, "provider")
                        .unwrap_or(existing.provider.clone());
                    let enabled = request
                        .arguments
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(existing.enabled);
                    let allows_guests = request
                        .arguments
                        .get("allows_guests")
                        .and_then(Value::as_bool)
                        .unwrap_or(existing.allows_guests);
                    let default_agent_id = request
                        .arguments
                        .get("default_agent_id")
                        .and_then(Value::as_str)
                        .map(Uri::parse)
                        .transpose()?
                        .or(existing.default_agent_id);
                    let settings = request
                        .arguments
                        .get("settings")
                        .cloned()
                        .unwrap_or(existing.settings);

                    db.upsert_port(
                        &port_name,
                        provider.as_str(),
                        enabled,
                        allows_guests,
                        default_agent_id.as_ref(),
                        &settings,
                    )
                    .await?;

                    let port = db
                        .get_port(&port_name)
                        .await?
                        .ok_or_else(|| anyhow!("port.not_found"))?;
                    json_text(json!({ "port": port }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Providers-listProviders")?,
            None,
            move |request| {
                let db = db_providers_list.clone();
                async move {
                    let limit = request
                        .arguments
                        .get("limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize;
                    let providers = db.list_providers(limit).await?;
                    json_text(json!({ "providers": providers }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Providers-createProvider")?,
            None,
            move |request| {
                let db = db_providers_create.clone();
                async move {
                    let provider = req_str(&request.arguments, "provider")?;
                    let api_key = req_str(&request.arguments, "api_key")?;
                    if db.get_provider(provider).await?.is_some() {
                        return Err(anyhow!("provider.already_exists"));
                    }
                    let enabled = request.arguments.get("enabled").and_then(Value::as_bool);
                    let default_text_model =
                        opt_trimmed_str(&request.arguments, "default_text_model");
                    let default_audio_model =
                        opt_trimmed_str(&request.arguments, "default_audio_model");

                    db.upsert_provider(
                        provider,
                        api_key,
                        enabled,
                        default_text_model.as_deref(),
                        default_audio_model.as_deref(),
                    )
                    .await?;

                    let out = db
                        .get_provider(provider)
                        .await?
                        .ok_or_else(|| anyhow!("provider.not_found"))?;
                    json_text(json!({ "provider": out }))
                }
            },
        ))?
        .add_tool(Tool::new(
            required_spec("Providers-updateProvider")?,
            None,
            move |request| {
                let db = db_providers_update.clone();
                async move {
                    let provider = req_str(&request.arguments, "provider")?;
                    let existing = db
                        .get_provider(provider)
                        .await?
                        .ok_or_else(|| anyhow!("provider.not_found"))?;

                    let api_key = opt_trimmed_str(&request.arguments, "api_key")
                        .unwrap_or(existing.api_key.clone());
                    let enabled = request
                        .arguments
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .or(Some(existing.enabled));
                    let default_text_model = request
                        .arguments
                        .get("default_text_model")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .map(ToOwned::to_owned)
                        .or(existing.default_text_model.clone())
                        .filter(|value| !value.is_empty());
                    let default_audio_model = request
                        .arguments
                        .get("default_audio_model")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .map(ToOwned::to_owned)
                        .or(existing.default_audio_model.clone())
                        .filter(|value| !value.is_empty());

                    db.upsert_provider(
                        provider,
                        api_key.as_str(),
                        enabled,
                        default_text_model.as_deref(),
                        default_audio_model.as_deref(),
                    )
                    .await?;

                    let out = db
                        .get_provider(provider)
                        .await?
                        .ok_or_else(|| anyhow!("provider.not_found"))?;
                    json_text(json!({ "provider": out }))
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
    default_admin_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("missing admin tool spec {}", name))
}

fn json_text(value: Value) -> Result<ToolResponse> {
    Ok(ToolResponse {
        content: ToolResultData::Text(serde_json::to_string(&value)?),
    })
}

fn req_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("validation_failed: missing {}", key))
}

fn opt_trimmed_str(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn port_name_from_uri(port_uri: &Uri) -> Result<String> {
    let raw = port_uri.to_string();
    let mut parts = raw.splitn(3, ':');
    let ns = parts.next().unwrap_or_default();
    let kind = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    if ns != "borg" || kind != "port" || id.trim().is_empty() {
        return Err(anyhow!(
            "validation_failed: port_uri must be borg:port:<name>"
        ));
    }
    Ok(id.to_string())
}
