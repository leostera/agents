use anyhow::Result;
use borg_core::Uri;
use clap::Subcommand;
use serde_json::{Value, json};

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum PortsCommand {
    #[command(about = "List ports")]
    List {
        #[arg(long, default_value_t = 100, help = "Maximum ports to return")]
        limit: usize,
    },
    #[command(about = "Get one port by name")]
    Get {
        #[arg(help = "Port name")]
        port_name: String,
    },
    #[command(about = "Create or update a port")]
    Upsert {
        #[arg(help = "Port name")]
        port_name: String,
        #[arg(long, help = "Port provider id")]
        provider: Option<String>,
        #[arg(long, help = "Enable or disable port")]
        enabled: Option<bool>,
        #[arg(long, help = "Allow or deny guest users")]
        allows_guests: Option<bool>,
        #[arg(long, help = "Default agent URI")]
        default_agent_id: Option<String>,
        #[arg(long, value_name = "JSON", help = "Port settings JSON object")]
        settings_json: Option<String>,
    },
    #[command(about = "Delete a port")]
    Delete {
        #[arg(help = "Port name")]
        port_name: String,
    },
}

pub async fn run(app: &BorgCliApp, cmd: PortsCommand) -> Result<()> {
    let db = app.open_config_db().await?;
    db.migrate().await?;

    let output = match cmd {
        PortsCommand::List { limit } => {
            let ports = db.list_ports(limit).await?;
            json!({ "ok": true, "entity": "ports", "items": ports })
        }
        PortsCommand::Get { port_name } => {
            let port = db.get_port(&port_name).await?;
            json!({ "ok": true, "entity": "ports", "item": port })
        }
        PortsCommand::Upsert {
            port_name,
            provider,
            enabled,
            allows_guests,
            default_agent_id,
            settings_json,
        } => {
            let existing = db.get_port(&port_name).await?;
            let provider = provider
                .or_else(|| existing.as_ref().map(|record| record.provider.clone()))
                .unwrap_or_else(|| "custom".to_string());
            let enabled = enabled
                .or_else(|| existing.as_ref().map(|record| record.enabled))
                .unwrap_or(true);
            let allows_guests = allows_guests
                .or_else(|| existing.as_ref().map(|record| record.allows_guests))
                .unwrap_or(true);

            let default_agent_id = match default_agent_id {
                Some(raw) if raw.trim().is_empty() => None,
                Some(raw) => Some(Uri::parse(raw.trim()).map_err(|_| {
                    anyhow::anyhow!("invalid default_agent_id uri `{}`", raw.trim())
                })?),
                None => existing
                    .as_ref()
                    .and_then(|record| record.default_agent_id.clone()),
            };

            let settings: Value = match settings_json {
                Some(raw) => serde_json::from_str(&raw).map_err(|err| {
                    anyhow::anyhow!("invalid settings_json: {} (payload={})", err, raw)
                })?,
                None => existing
                    .as_ref()
                    .map(|record| record.settings.clone())
                    .unwrap_or_else(|| json!({})),
            };

            db.upsert_port(
                &port_name,
                &provider,
                enabled,
                allows_guests,
                default_agent_id.as_ref(),
                &settings,
            )
            .await?;

            let port = db.get_port(&port_name).await?;
            json!({ "ok": true, "entity": "ports", "item": port })
        }
        PortsCommand::Delete { port_name } => {
            db.delete_port(&port_name).await?;
            json!({ "ok": true, "entity": "ports", "deleted": 1, "port_name": port_name })
        }
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
