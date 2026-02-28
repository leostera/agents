use anyhow::Result;
use borg_agent::ToolSpec;
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::Uri;
use borg_db::BorgDb;
use borg_memory::default_memory_tool_specs;

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
            apps: vec![build_codemode_app(), build_memory_app()],
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
