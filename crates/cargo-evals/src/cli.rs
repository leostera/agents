use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::cmd::{list, run};
use crate::config::EvalsFile;
use crate::discovery::discover_eval_crates;

#[derive(Debug, Parser)]
#[command(name = "cargo-evals")]
#[command(bin_name = "cargo evals")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    List,
    Run,
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        let workspace_root = std::env::current_dir().context("resolve workspace root")?;
        let evals_file = EvalsFile::load(&workspace_root)?;
        let crates = discover_eval_crates(&workspace_root);

        match self.command {
            Command::List => list::run(&evals_file, &crates)?,
            Command::Run => run::run(&evals_file, &crates).await?,
        }

        Ok(())
    }
}
