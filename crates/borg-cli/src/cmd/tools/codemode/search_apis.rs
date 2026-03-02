use anyhow::Result;
use serde_json::Value;

#[derive(clap::Args, Debug)]
#[command(about = "Search available Borg SDK APIs")]
pub struct SearchApisArgs {
    #[arg(long, help = "Fuzzy API search query")]
    pub query: Option<String>,
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

pub async fn run(args: SearchApisArgs) -> Result<Value> {
    let payload = if let Some(raw) = args.payload_json {
        raw
    } else {
        serde_json::to_string(&serde_json::json!({
            "query": args.query.unwrap_or_default(),
        }))?
    };
    super::run_search_apis(&payload).await
}
