use anyhow::{Result, anyhow};
use borg_agent::{BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_core::Uri;
use borg_db::BorgDb;
use serde_json::{Value, json};

pub fn default_port_admin_tool_specs() -> Vec<ToolSpec> {
    vec![
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
    ]
}

pub fn build_port_admin_toolchain(db: BorgDb) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let db_list = db.clone();
    let db_create = db.clone();
    let db_update = db;

    Toolchain::builder()
        .add_tool(Tool::new(
            required_spec("Ports-listPorts")?,
            None,
            move |request| {
                let db = db_list.clone();
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
                let db = db_create.clone();
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
                let db = db_update.clone();
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
    default_port_admin_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("missing port admin tool spec {}", name))
}

fn json_text(value: Value) -> Result<ToolResponse<Value>> {
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
