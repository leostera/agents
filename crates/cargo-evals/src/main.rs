mod cli;
mod cmd;
mod config;
mod discovery;
mod harness;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "evals") {
        args.remove(1);
    }

    Cli::parse_from(args).run().await
}
