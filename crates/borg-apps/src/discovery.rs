use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult, Tool, ToolRequest, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_db::{AppCapabilityRecord, AppRecord, BorgDb};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct BorgApps {
    apps: Vec<AppCatalogEntry>,
}

#[derive(Debug, Clone)]
struct AppCatalogEntry {
    app: AppRecord,
    capabilities: Vec<AppCapabilityRecord>,
}

impl AppCatalogEntry {
    fn summary(&self) -> String {
        summarize_text(&self.app.description, 140)
    }

    fn capability_description(capability: &AppCapabilityRecord) -> String {
        let instructions = capability.instructions.trim();
        if !instructions.is_empty() {
            return instructions.to_string();
        }
        capability.hint.trim().to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppCatalogItem {
    pub app_id: String,
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityCatalogItem {
    pub app_id: String,
    pub app_name: String,
    pub app_slug: String,
    pub app_status: String,
    pub app_built_in: bool,
    pub capability_id: String,
    pub capability_name: String,
    pub capability_hint: String,
    pub capability_mode: String,
    pub capability_instructions: String,
    pub capability_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppCapabilityDetail {
    pub capability_id: String,
    pub name: String,
    pub mode: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppDetailsResult {
    pub app_id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub built_in: bool,
    pub source: String,
    pub auth_strategy: String,
    pub auth_config_json: serde_json::Value,
    pub available_secrets: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub capabilities: Vec<AppCapabilityDetail>,
}

impl BorgApps {
    pub async fn new(db: BorgDb) -> Result<Self> {
        let apps = db.list_apps(500).await?;
        let mut entries = Vec::with_capacity(apps.len());
        for app in apps {
            let capabilities = db.list_app_capabilities(&app.app_id, 500).await?;
            entries.push(AppCatalogEntry { app, capabilities });
        }
        Ok(Self { apps: entries })
    }

    pub fn as_toolchain(&self) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
        let app_items = self.list_apps();
        let app_details = self.list_app_details();
        Toolchain::builder()
            .add_tool(Tool::new(
                apps_list_apps_tool_spec(),
                None,
                move |_request: ToolRequest<Value>| {
                    let items = app_items.clone();
                    async move {
                        Ok(ToolResponse {
                            content: ToolResultData::Text(serde_json::to_string(&json!({
                                "apps": items
                            }))?),
                        })
                    }
                },
            ))?
            .add_tool(Tool::new(
                apps_get_app_tool_spec(),
                None,
                move |request: ToolRequest<Value>| {
                    let entries = app_details.clone();
                    async move {
                        let id = request
                            .arguments
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| anyhow::anyhow!("missing required field: id"))?;
                        let app = entries
                            .iter()
                            .find(|entry| entry.app_id == id)
                            .ok_or_else(|| anyhow::anyhow!("app not found: {id}"))?;
                        Ok(ToolResponse {
                            content: ToolResultData::Text(serde_json::to_string(&json!({
                                "app": app
                            }))?),
                        })
                    }
                },
            ))?
            .build()
    }

    pub fn list_apps(&self) -> Vec<AppCatalogItem> {
        self.apps
            .iter()
            .map(|entry| AppCatalogItem {
                app_id: entry.app.app_id.to_string(),
                name: entry.app.name.clone(),
                summary: entry.summary(),
            })
            .collect()
    }

    pub fn list_app_details(&self) -> Vec<AppDetailsResult> {
        self.apps
            .iter()
            .map(|entry| AppDetailsResult {
                app_id: entry.app.app_id.to_string(),
                name: entry.app.name.clone(),
                slug: entry.app.slug.clone(),
                description: entry.app.description.clone(),
                status: entry.app.status.clone(),
                built_in: entry.app.built_in,
                source: entry.app.source.clone(),
                auth_strategy: entry.app.auth_strategy.clone(),
                auth_config_json: entry.app.auth_config_json.clone(),
                available_secrets: entry.app.available_secrets.clone(),
                created_at: entry.app.created_at,
                updated_at: entry.app.updated_at,
                capabilities: entry
                    .capabilities
                    .iter()
                    .map(|capability| AppCapabilityDetail {
                        capability_id: capability.capability_id.to_string(),
                        name: capability.name.clone(),
                        mode: capability.mode.clone(),
                        description: AppCatalogEntry::capability_description(capability),
                        status: capability.status.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn list_capabilities(&self) -> Vec<CapabilityCatalogItem> {
        let mut items = Vec::new();
        for entry in &self.apps {
            for capability in &entry.capabilities {
                items.push(CapabilityCatalogItem {
                    app_id: entry.app.app_id.to_string(),
                    app_name: entry.app.name.clone(),
                    app_slug: entry.app.slug.clone(),
                    app_status: entry.app.status.clone(),
                    app_built_in: entry.app.built_in,
                    capability_id: capability.capability_id.to_string(),
                    capability_name: capability.name.clone(),
                    capability_hint: capability.hint.clone(),
                    capability_mode: capability.mode.clone(),
                    capability_instructions: AppCatalogEntry::capability_description(capability),
                    capability_status: capability.status.clone(),
                });
            }
        }
        items
    }

    pub fn capability_tool_specs(&self) -> Vec<ToolSpec> {
        self.list_capabilities()
            .into_iter()
            .filter(|capability| {
                capability.app_status.eq_ignore_ascii_case("active")
                    && capability.capability_status.eq_ignore_ascii_case("active")
            })
            .map(|capability| ToolSpec {
                name: capability.capability_name,
                description: format!(
                    "App capability from {} (mode: {}). {}\n\n{}",
                    capability.app_name,
                    capability.capability_mode,
                    capability.capability_hint,
                    capability.capability_instructions
                ),
                parameters: json!({
                    "type": "object",
                    "additionalProperties": true
                }),
            })
            .collect()
    }
}

pub fn apps_list_apps_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "Apps-listApps".to_string(),
        description: r#"
Lists all apps available to the runtime.
Returns each app name and a short summary of what the app is about.
Use this first to pick an app id.
If you want to use a listed app, call `Apps-getApp(id)` with that app's `app_id` before using its capabilities.
"#
        .trim()
        .to_string(),
        parameters: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
    }
}

pub fn apps_get_app_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "Apps-getApp".to_string(),
        description: r#"
Returns full details for one app by id.
Includes the app description and detailed descriptions of each capability available in that app.
Includes the `available_secrets` list that may be exposed in `Borg.env` when the app is connected.
"#
        .trim()
        .to_string(),
        parameters: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "id": { "type": "string" }
            },
            "required": ["id"]
        }),
    }
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![apps_list_apps_tool_spec(), apps_get_app_tool_spec()]
}

fn summarize_text(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= limit {
        return normalized;
    }
    let mut out = String::with_capacity(limit + 1);
    for ch in normalized.chars().take(limit) {
        out.push(ch);
    }
    out.push('…');
    out
}
