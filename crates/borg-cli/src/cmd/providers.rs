use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum ProvidersCommand {
    #[command(about = "List providers")]
    List {
        #[arg(long, default_value_t = 100, help = "Maximum providers to return")]
        limit: usize,
    },
    #[command(about = "Get one provider by id")]
    Get {
        #[arg(help = "Provider id (for example openai or openrouter)")]
        provider: String,
    },
    #[command(about = "Create or update a provider")]
    Upsert {
        #[arg(help = "Provider id")]
        provider: String,
        #[arg(long, help = "Provider kind (openai|openrouter|lmstudio|ollama)")]
        provider_kind: Option<String>,
        #[arg(long, help = "Provider API key (required when creating)")]
        api_key: Option<String>,
        #[arg(long, help = "Provider base URL")]
        base_url: Option<String>,
        #[arg(long, help = "Enable or disable provider")]
        enabled: Option<bool>,
        #[arg(long, help = "Default text model")]
        default_text_model: Option<String>,
        #[arg(long, help = "Default audio model")]
        default_audio_model: Option<String>,
    },
    #[command(about = "Delete a provider")]
    Delete {
        #[arg(help = "Provider id")]
        provider: String,
    },
}

pub async fn run(app: &BorgCliApp, cmd: ProvidersCommand) -> Result<()> {
    let db = app.open_config_db().await?;
    db.migrate().await?;

    let output = match cmd {
        ProvidersCommand::List { limit } => {
            let providers = db.list_providers(limit).await?;
            json!({ "ok": true, "entity": "providers", "items": providers })
        }
        ProvidersCommand::Get { provider } => {
            let provider_record = db.get_provider(&provider).await?;
            json!({ "ok": true, "entity": "providers", "item": provider_record })
        }
        ProvidersCommand::Upsert {
            provider,
            provider_kind,
            api_key,
            base_url,
            enabled,
            default_text_model,
            default_audio_model,
        } => {
            let provider_kind = provider_kind.unwrap_or_else(|| provider.clone());
            let api_key_to_use = match api_key {
                Some(key) => key,
                None => db
                    .get_provider(&provider)
                    .await?
                    .map(|record| record.api_key)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "api_key is required when creating a new provider `{}`",
                            provider
                        )
                    })?,
            };

            db.upsert_provider_with_kind(
                &provider,
                &provider_kind,
                Some(&api_key_to_use),
                base_url.as_deref(),
                enabled,
                default_text_model.as_deref(),
                default_audio_model.as_deref(),
            )
            .await?;
            let provider_record = db.get_provider(&provider).await?;
            json!({ "ok": true, "entity": "providers", "item": provider_record })
        }
        ProvidersCommand::Delete { provider } => {
            let deleted = db.delete_provider(&provider).await?;
            json!({ "ok": true, "entity": "providers", "deleted": deleted })
        }
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
