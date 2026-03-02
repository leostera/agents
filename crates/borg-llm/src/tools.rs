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
                    "provider_kind": { "type": "string" },
                    "api_key": { "type": "string" },
                    "base_url": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "default_text_model": { "type": "string" },
                    "default_audio_model": { "type": "string" }
                },
                "required": ["provider"],
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
                    "provider_kind": { "type": "string" },
                    "api_key": { "type": "string" },
                    "base_url": { "type": "string" },
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
            let provider = normalize_provider_id(req_str(arguments, "provider")?)?;
            let provider_kind = opt_trimmed_str(arguments, "provider_kind")
                .unwrap_or_else(|| provider.to_string());
            let api_key = opt_trimmed_str(arguments, "api_key");
            let base_url = opt_trimmed_str(arguments, "base_url");
            validate_provider_config(
                &provider_kind,
                api_key.as_deref(),
                base_url.as_deref(),
            )?;
            if db.get_provider(provider).await?.is_some() {
                return Err(anyhow!("provider.already_exists"));
            }
            let enabled = arguments.get("enabled").and_then(Value::as_bool);
            let default_text_model = opt_trimmed_str(arguments, "default_text_model");
            let default_audio_model = opt_trimmed_str(arguments, "default_audio_model");

            db.upsert_provider_with_kind(
                provider,
                &provider_kind,
                api_key.as_deref(),
                base_url.as_deref(),
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
            let provider = normalize_provider_id(req_str(arguments, "provider")?)?;
            let existing = db
                .get_provider(provider)
                .await?
                .ok_or_else(|| anyhow!("provider.not_found"))?;
            let provider_kind = opt_trimmed_str(arguments, "provider_kind")
                .unwrap_or(existing.provider_kind.clone());

            let api_key = opt_trimmed_str(arguments, "api_key")
                .or_else(|| normalize_existing_text(&existing.api_key));
            let base_url = arguments
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(ToOwned::to_owned)
                .or(existing.base_url.clone())
                .filter(|value| !value.is_empty());
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
            validate_provider_config(&provider_kind, api_key.as_deref(), base_url.as_deref())?;

            db.upsert_provider_with_kind(
                provider,
                &provider_kind,
                api_key.as_deref(),
                base_url.as_deref(),
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

fn normalize_existing_text(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn normalize_provider_id(raw: &str) -> Result<&str> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(anyhow!("validation_failed: missing provider"));
    }
    Ok(normalized)
}

fn validate_provider_config(
    provider: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    match provider {
        "openai" | "openrouter" => {
            if api_key.is_none() {
                return Err(anyhow!("validation_failed: missing api_key"));
            }
        }
        "lmstudio" | "ollama" => {
            if base_url.is_none() {
                return Err(anyhow!("validation_failed: missing base_url"));
            }
        }
        _ => return Err(anyhow!("provider.not_supported")),
    }
    Ok(())
}
