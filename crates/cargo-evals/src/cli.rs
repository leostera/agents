use std::ffi::OsString;

use anyhow::{Context, Result};
use borg_evals::{
    TargetFilter,
    runner::{RunOptions, list_models_workspace, list_workspace, run_workspace},
};
use clap::Parser;

#[derive(Debug)]
pub enum Cli {
    List(ListArgs),
    Models,
    Run(RunArgs),
}

#[derive(Debug, Parser, Clone)]
#[command(name = "cargo-evals-list")]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Parser, Clone, Default)]
#[command(name = "cargo-evals-run")]
pub struct RunArgs {
    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub model: Option<String>,

    pub query: Option<String>,
}

impl Cli {
    pub fn parse_from(args: Vec<OsString>) -> Self {
        let command = args.get(1).and_then(|arg| arg.to_str()).map(str::to_string);

        match command.as_deref() {
            Some("list") => {
                let mut list_args = vec![args[0].clone()];
                list_args.extend(args.into_iter().skip(2));
                Self::List(ListArgs::parse_from(list_args))
            }
            Some("models") => Self::Models,
            Some("run") => {
                let mut run_args = vec![args[0].clone()];
                run_args.extend(args.into_iter().skip(2));
                Self::Run(RunArgs::parse_from(run_args))
            }
            _ => Self::Run(RunArgs::parse_from(args)),
        }
    }

    pub async fn run(self) -> Result<()> {
        let workspace_root = std::env::current_dir().context("resolve workspace root")?;

        match self {
            Cli::List(args) => list_workspace(
                &workspace_root,
                RunOptions {
                    json: args.json,
                    ..RunOptions::default()
                },
            )?,
            Cli::Models => list_models_workspace(&workspace_root)?,
            Cli::Run(args) => {
                run_workspace(
                    &workspace_root,
                    RunOptions {
                        json: args.json,
                        filter: TargetFilter {
                            query: args.query,
                            model: args.model,
                        },
                    },
                )
                .await?
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Cli, ListArgs, RunArgs};

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn bare_command_defaults_to_run() {
        let cli = Cli::parse_from(args(&["cargo-evals"]));

        match cli {
            Cli::Run(RunArgs { json, model, query }) => {
                assert!(!json);
                assert!(model.is_none());
                assert!(query.is_none());
            }
            Cli::List(_) | Cli::Models => panic!("expected run args"),
        }
    }

    #[test]
    fn bare_query_defaults_to_run_filter() {
        let cli = Cli::parse_from(args(&["cargo-evals", "preserves"]));

        match cli {
            Cli::Run(RunArgs { query, .. }) => {
                assert_eq!(query.as_deref(), Some("preserves"));
            }
            Cli::List(_) | Cli::Models => panic!("expected run args"),
        }
    }

    #[test]
    fn run_subcommand_accepts_model_and_query() {
        let cli = Cli::parse_from(args(&[
            "cargo-evals",
            "run",
            "--model",
            "ollama/llama3.2:1b",
            "preserves",
        ]));

        match cli {
            Cli::Run(RunArgs { model, query, .. }) => {
                assert_eq!(model.as_deref(), Some("ollama/llama3.2:1b"));
                assert_eq!(query.as_deref(), Some("preserves"));
            }
            Cli::List(_) | Cli::Models => panic!("expected run args"),
        }
    }

    #[test]
    fn list_subcommand_parses_json_flag() {
        let cli = Cli::parse_from(args(&["cargo-evals", "list", "--json"]));

        match cli {
            Cli::List(ListArgs { json }) => assert!(json),
            Cli::Run(_) | Cli::Models => panic!("expected list args"),
        }
    }

    #[test]
    fn models_subcommand_is_recognized() {
        match Cli::parse_from(args(&["cargo-evals", "models"])) {
            Cli::Models => {}
            Cli::List(_) | Cli::Run(_) => panic!("expected models command"),
        }
    }
}
