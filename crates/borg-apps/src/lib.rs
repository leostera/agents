use anyhow::Result;
use borg_agent::{Tool, ToolRequest, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::Uri;
use borg_db::{AppCapabilityRecord, AppRecord, BorgDb};
use borg_memory::default_memory_tool_specs;
use borg_shellmode::default_tool_specs as default_shellmode_tool_specs;
use borg_taskgraph::default_taskgraph_tool_specs;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct DefaultCapability {
    pub capability_id: String,
    pub name: String,
    pub hint: String,
    pub mode: String,
    pub instructions: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct DefaultApp {
    pub app_id: &'static str,
    pub name: &'static str,
    pub slug: &'static str,
    pub description: &'static str,
    pub status: &'static str,
    pub capabilities: Vec<DefaultCapability>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallSummary {
    pub apps_created: usize,
    pub capabilities_created: usize,
}

pub struct DefaultAppsCatalog {
    apps: Vec<DefaultApp>,
}

impl DefaultAppsCatalog {
    pub fn new() -> Self {
        Self {
            apps: vec![
                build_codemode_app(),
                build_shellmode_app(),
                build_memory_app(),
                build_taskgraph_app(),
            ],
        }
    }

    pub async fn install_missing(&self, db: &BorgDb) -> Result<InstallSummary> {
        let mut summary = InstallSummary::default();

        for app in &self.apps {
            let app_uri = Uri::parse(app.app_id)?;
            if db.get_app(&app_uri).await?.is_some() {
                continue;
            }

            db.upsert_builtin_app(&app_uri, app.name, app.slug, app.description, app.status)
                .await?;
            summary.apps_created += 1;

            for capability in &app.capabilities {
                let capability_uri = Uri::parse(&capability.capability_id)?;
                db.upsert_app_capability(
                    &app_uri,
                    &capability_uri,
                    &capability.name,
                    &capability.hint,
                    &capability.mode,
                    &capability.instructions,
                    &capability.status,
                )
                .await?;
                summary.capabilities_created += 1;
            }
        }

        Ok(summary)
    }
}

impl Default for DefaultAppsCatalog {
    fn default() -> Self {
        Self::new()
    }
}

fn build_codemode_app() -> DefaultApp {
    DefaultApp {
        app_id: "borg:app:codemode-runtime",
        name: "CodeMode Runtime",
        slug: "codemode-runtime",
        description: "Core JavaScript execution app for sandboxed runtime capabilities and API composition.",
        status: "active",
        capabilities: tool_specs_to_capabilities(
            "codemode-runtime",
            "codemode",
            default_codemode_tool_specs(),
        ),
    }
}

fn build_shellmode_app() -> DefaultApp {
    DefaultApp {
        app_id: "borg:app:shellmode-runtime",
        name: "ShellMode Runtime",
        slug: "shellmode-runtime",
        description: "Core shell execution app for running arbitrary commands on the host system.",
        status: "active",
        capabilities: tool_specs_to_capabilities(
            "shellmode-runtime",
            "shell",
            default_shellmode_tool_specs(),
        ),
    }
}

fn build_memory_app() -> DefaultApp {
    DefaultApp {
        app_id: "borg:app:memory-system",
        name: "Memory System",
        slug: "memory-system",
        description: "Long-term memory app for searching and writing facts used by agents across sessions.",
        status: "active",
        capabilities: tool_specs_to_capabilities(
            "memory-system",
            "codemode",
            default_memory_tool_specs(),
        ),
    }
}

fn build_taskgraph_app() -> DefaultApp {
    DefaultApp {
        app_id: "borg:app:taskgraph-system",
        name: "TaskGraph System",
        slug: "taskgraph-system",
        description: "Durable task DAG app with review-aware workflow, dependency management, and queue tooling.",
        status: "active",
        capabilities: tool_specs_to_capabilities(
            "taskgraph-system",
            "codemode",
            default_taskgraph_tool_specs(),
        ),
    }
}

fn tool_specs_to_capabilities(
    app_slug: &str,
    mode: &str,
    specs: Vec<ToolSpec>,
) -> Vec<DefaultCapability> {
    specs
        .into_iter()
        .map(|spec| DefaultCapability {
            capability_id: capability_uri_from_tool_name(app_slug, &spec.name),
            name: spec.name,
            hint: spec.description.clone(),
            mode: mode.to_string(),
            instructions: spec.description,
            status: "active".to_string(),
        })
        .collect()
}

fn capability_uri_from_tool_name(app_slug: &str, tool_name: &str) -> String {
    let normalized_tool_name = tool_name
        .trim()
        .to_ascii_lowercase()
        .replace([' ', ':', '/'], "-");
    format!("borg:capability:{app_slug}-{normalized_tool_name}")
}

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
    app_id: String,
    app_name: String,
    app_slug: String,
    app_status: String,
    app_built_in: bool,
    capability_id: String,
    capability_name: String,
    capability_hint: String,
    capability_mode: String,
    capability_instructions: String,
    capability_status: String,
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
