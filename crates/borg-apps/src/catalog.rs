use anyhow::Result;
use borg_agent::ToolSpec;
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::{Config, Uri};
use borg_db::BorgDb;
use borg_memory::default_memory_tool_specs;
use borg_schedule::default_schedule_tool_specs;
use borg_shellmode::default_tool_specs as default_shellmode_tool_specs;
use borg_taskgraph::default_taskgraph_tool_specs;
use serde_json::{Value, json};

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
    pub auth_strategy: &'static str,
    pub auth_config_json: Value,
    pub available_secrets: Vec<String>,
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
                Self::build_codemode_app(),
                Self::build_shellmode_app(),
                Self::build_memory_app(),
                Self::build_taskgraph_app(),
                Self::build_schedule_app(),
                Self::build_github_app(),
            ],
        }
    }

    pub async fn install_missing(&self, db: &BorgDb) -> Result<InstallSummary> {
        let mut summary = InstallSummary::default();

        for app in &self.apps {
            let app_uri = Uri::parse(app.app_id)?;
            let existed = db.get_app(&app_uri).await?.is_some();

            db.upsert_app_with_metadata(
                &app_uri,
                app.name,
                app.slug,
                app.description,
                app.status,
                true,
                "managed",
                app.auth_strategy,
                &app.auth_config_json,
                &app.available_secrets,
            )
            .await?;
            if !existed {
                summary.apps_created += 1;
            }

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
                if !existed {
                    summary.capabilities_created += 1;
                }
            }
        }

        Ok(summary)
    }

    fn build_codemode_app() -> DefaultApp {
        DefaultApp {
            app_id: "borg:app:codemode-runtime",
            name: "CodeMode Runtime",
            slug: "codemode-runtime",
            description: "Core JavaScript execution app for sandboxed runtime capabilities and API composition.",
            status: "active",
            auth_strategy: "none",
            auth_config_json: json!({}),
            available_secrets: Vec::new(),
            capabilities: Self::tool_specs_to_capabilities(
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
            auth_strategy: "none",
            auth_config_json: json!({}),
            available_secrets: Vec::new(),
            capabilities: Self::tool_specs_to_capabilities(
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
            auth_strategy: "none",
            auth_config_json: json!({}),
            available_secrets: Vec::new(),
            capabilities: Self::tool_specs_to_capabilities(
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
            auth_strategy: "none",
            auth_config_json: json!({}),
            available_secrets: Vec::new(),
            capabilities: Self::tool_specs_to_capabilities(
                "taskgraph-system",
                "codemode",
                default_taskgraph_tool_specs(),
            ),
        }
    }

    fn build_schedule_app() -> DefaultApp {
        DefaultApp {
            app_id: "borg:app:schedule-system",
            name: "Schedule System",
            slug: "schedule-system",
            description: "Durable scheduler app for creating and managing one-shot and cron jobs that deliver Borg chat messages to actor sessions.",
            status: "active",
            auth_strategy: "none",
            auth_config_json: json!({}),
            available_secrets: Vec::new(),
            capabilities: Self::tool_specs_to_capabilities(
                "schedule-system",
                "codemode",
                default_schedule_tool_specs(),
            ),
        }
    }

    fn build_github_app() -> DefaultApp {
        let config = Config::default();
        let mut auth_config_json = json!({
            "authorize_url": "https://github.com/login/oauth/authorize",
            "token_url": "https://github.com/login/oauth/access_token",
            "client_id": config.github_oauth_client_id,
            "scopes": ["read:user", "repo"],
            "userinfo_url": "https://api.github.com/user"
        });
        if let Some(client_secret) = config.github_oauth_client_secret {
            auth_config_json["client_secret"] = Value::String(client_secret);
        }
        DefaultApp {
            app_id: "borg:app:github",
            name: "GitHub",
            slug: "github",
            description:
                "Connect to GitHub via OAuth to read repositories and collaborate on issues and pull requests.",
            status: "active",
            auth_strategy: "oauth2",
            auth_config_json,
            available_secrets: vec![
                "GITHUB_ACCESS_TOKEN".to_string(),
                "GITHUB_REFRESH_TOKEN".to_string(),
                "GITHUB_SCOPE".to_string(),
                "GITHUB_EXPIRES_AT".to_string(),
            ],
            capabilities: vec![DefaultCapability {
                capability_id: "borg:capability:github-list-repos".to_string(),
                name: "GitHub-listRepos".to_string(),
                hint: "List repositories available to the connected GitHub account.".to_string(),
                mode: "codemode".to_string(),
                instructions: r#"
Use this capability when you need the user's repositories from GitHub.
The OAuth access token is expected to be available on the app connection secrets.
Call `GET https://api.github.com/user/repos` with `Authorization: Bearer <token>` and a `User-Agent` header.
"#
                .trim()
                .to_string(),
                status: "active".to_string(),
            }],
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
                capability_id: Self::capability_uri_from_tool_name(app_slug, &spec.name),
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
}

impl Default for DefaultAppsCatalog {
    fn default() -> Self {
        Self::new()
    }
}
