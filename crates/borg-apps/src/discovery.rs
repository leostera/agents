use anyhow::Result;
use borg_agent::{Tool, ToolRequest, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_db::{AppCapabilityRecord, AppRecord, BorgDb};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct BorgApps {
    apps: Vec<AppCatalogEntry>,
}

#[derive(Debug, Clone)]
struct AppCatalogEntry {
    app: AppRecord,
    capabilities: Vec<AppCapabilityRecord>,
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

    pub fn as_toolchain(&self) -> Result<Toolchain> {
        let catalog_items = self.list_capabilities();
        let tool = Tool::new(
            apps_list_capabilities_tool_spec(),
            None,
            move |_request: ToolRequest| {
                let items = catalog_items.clone();
                async move {
                    Ok(ToolResponse {
                        content: ToolResultData::Text(serde_json::to_string(&json!({
                            "capabilities": items
                        }))?),
                    })
                }
            },
        );
        Toolchain::builder().add_tool(tool)?.build()
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
                    capability_instructions: capability.instructions.clone(),
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

pub fn apps_list_capabilities_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "Apps-listCapabilities".to_string(),
        description: r#"
Lists all app capabilities currently defined in the database.
Use this to discover what integrations/capabilities exist before deciding what to call next.
Returns app and capability metadata, including mode and instructions.
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

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![apps_list_capabilities_tool_spec()]
}
