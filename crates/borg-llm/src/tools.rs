use anyhow::{Result, anyhow};
use borg_db::BorgDb;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct ProviderAdminToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub fn default_provider_admin_tool_specs() -> Vec<ProviderAdminToolSpec> {
    vec![
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

pub async fn run_provider_admin_tool(db: &BorgDb, name: &str, arguments: &Value) -> Result<Value> {
    match name {
        "Providers-listProviders" => {
            let limit = arguments
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(100) as usize;
            let providers = db.list_providers(limit).await?;
            Ok(json!({ "providers": providers }))
        }
        "Providers-createProvider" => {
            let provider = req_str(arguments, "provider")?;
            let api_key = req_str(arguments, "api_key")?;
            if db.get_provider(provider).await?.is_some() {
                return Err(anyhow!("provider.already_exists"));
            }
            let enabled = arguments.get("enabled").and_then(Value::as_bool);
            let default_text_model = opt_trimmed_str(arguments, "default_text_model");
            let default_audio_model = opt_trimmed_str(arguments, "default_audio_model");

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
            Ok(json!({ "provider": out }))
        }
        "Providers-updateProvider" => {
            let provider = req_str(arguments, "provider")?;
            let existing = db
                .get_provider(provider)
                .await?
                .ok_or_else(|| anyhow!("provider.not_found"))?;

            let api_key = opt_trimmed_str(arguments, "api_key").unwrap_or(existing.api_key.clone());
            let enabled = arguments
                .get("enabled")
                .and_then(Value::as_bool)
                .or(Some(existing.enabled));
            let default_text_model = arguments
                .get("default_text_model")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(ToOwned::to_owned)
                .or(existing.default_text_model.clone())
                .filter(|value| !value.is_empty());
            let default_audio_model = arguments
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
            Ok(json!({ "provider": out }))
        }
        _ => Err(anyhow!("unknown provider admin tool {name}")),
    }
}

fn tool_spec(name: &str, description: &str, parameters: Value) -> ProviderAdminToolSpec {
    ProviderAdminToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
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
