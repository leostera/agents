use anyhow::{Context, Result};
use borg_evals::runner::{RunOptions, list_workspace, run_workspace};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cargo-evals")]
#[command(bin_name = "cargo evals")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    List(ListArgs),
    Run(RunArgs),
}

#[derive(Debug, Args, Clone, Copy)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args, Clone, Copy)]
pub struct RunArgs {
    #[arg(long)]
    pub json: bool,
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        let workspace_root = std::env::current_dir().context("resolve workspace root")?;

        match self.command {
            Command::List(args) => list_workspace(&workspace_root, RunOptions { json: args.json })?,
            Command::Run(args) => {
                run_workspace(&workspace_root, RunOptions { json: args.json }).await?
            }
        }

        Ok(())
    }
}
