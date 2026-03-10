use anyhow::Result;
use serde_json::Value;

#[derive(clap::Args, Debug)]
#[command(about = "Execute JavaScript in the CodeMode runtime")]
pub struct ExecuteCodeArgs {
    #[arg(long, help = "Code snippet in async arrow-function form")]
    pub code: Option<String>,
    #[arg(long, help = "Short human-readable execution hint")]
    pub hint: Option<String>,
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

pub async fn run(args: ExecuteCodeArgs) -> Result<Value> {
    let payload = if let Some(raw) = args.payload_json {
        raw
    } else {
        serde_json::to_string(&serde_json::json!({
            "code": args.code.unwrap_or_default(),
            "hint": args.hint.unwrap_or_default(),
        }))?
    };
    super::run_execute_code(&payload).await
}
