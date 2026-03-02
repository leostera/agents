mod app;
mod cmd;

use anyhow::Result;
use borg_core::borgdir::BorgDir;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| {
            "info,borg_cli=debug,borg_api=debug,borg_ports=debug,borg_db=debug,borg_exec=debug,borg_memory=debug,borg_codemode=debug"
                .to_string()
        }))
        .with_writer(std::io::stderr)
        .init();

    let borg_dir = BorgDir::new();
    borg_dir.ensure_initialized().await?;

    let cli = cmd::Cli::parse();
    let app = app::BorgCliApp::new(borg_dir);
    cmd::run(app, cli).await
}
