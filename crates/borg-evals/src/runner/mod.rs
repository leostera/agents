mod config;
mod discovery;
mod harness;

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{
    EventSink, JsonEventSink, ProgressEventSink, RunConfig, SuiteDescriptor, set_global_sink,
};
use config::EvalsFile;
use discovery::discover_eval_crates;
use harness::generate_harness;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub json: bool,
}

pub fn list_workspace(workspace_root: &Path, options: RunOptions) -> Result<()> {
    let evals_file = EvalsFile::load(workspace_root)?;
    let crates = discover_eval_crates(workspace_root);
    let harness = generate_harness(workspace_root, &evals_file, &crates)?;

    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("--quiet")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(harness.manifest_path())
        .arg("--")
        .arg("list");

    if options.json {
        command.arg("--json");
    }

    let status = command
        .status()
        .context("run generated cargo-evals harness list")?;

    if !status.success() {
        bail!("generated cargo-evals harness list failed");
    }

    Ok(())
}

pub async fn run_workspace(workspace_root: &Path, options: RunOptions) -> Result<()> {
    let evals_file = EvalsFile::load(workspace_root)?;
    let crates = discover_eval_crates(workspace_root);
    let harness = generate_harness(workspace_root, &evals_file, &crates)?;

    print!("Preparing evals... ");
    std::io::Write::flush(&mut std::io::stdout()).context("flush prepare message")?;

    let build_status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(harness.manifest_path())
        .status()
        .context("build generated cargo-evals harness")?;

    if !build_status.success() {
        println!("FAILED");
        bail!("generated cargo-evals harness build failed");
    }

    println!("DONE!");

    let mut command = Command::new(harness.binary_path());
    command.arg("run");
    if options.json {
        command.arg("--json");
    }

    let status = command
        .status()
        .context("run generated cargo-evals harness")?;

    if !status.success() {
        bail!("generated cargo-evals harness failed");
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct ListedEval<'a> {
    id: &'a str,
}

#[derive(Debug, Serialize)]
struct ListedSuite<'a> {
    id: &'a str,
    evals: Vec<ListedEval<'a>>,
}

#[derive(Debug, Serialize)]
struct ListedCrate<'a> {
    name: &'a str,
    suites: Vec<ListedSuite<'a>>,
}

#[derive(Debug, Serialize)]
struct ListedTarget<'a> {
    label: &'a str,
    provider: &'a str,
    model: &'a str,
    concurrency: usize,
}

#[derive(Debug, Serialize)]
struct ListedOutput<'a> {
    crates: Vec<ListedCrate<'a>>,
    targets: Vec<ListedTarget<'a>>,
}

pub fn list_discovered(
    registries: &[(&str, Vec<SuiteDescriptor>)],
    run_config: &RunConfig,
    json: bool,
) {
    if json {
        let payload = ListedOutput {
            crates: registries
                .iter()
                .map(|(crate_name, suites)| ListedCrate {
                    name: crate_name,
                    suites: suites
                        .iter()
                        .map(|suite| ListedSuite {
                            id: suite.id,
                            evals: suite
                                .eval_ids
                                .iter()
                                .map(|eval_id| ListedEval { id: eval_id })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            targets: run_config
                .targets
                .iter()
                .map(|target| ListedTarget {
                    label: &target.label,
                    provider: &target.provider,
                    model: &target.model,
                    concurrency: target.max_in_flight,
                })
                .collect(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).expect("serialize eval list payload")
        );
        return;
    }

    for (crate_name, suites) in registries {
        println!("crate {}", crate_name);
        for suite in suites {
            println!("suite {}", suite.id);
            for eval_id in suite.eval_ids {
                println!("  eval {}", eval_id);
            }
        }
    }
}

pub async fn run_discovered(
    registries: Vec<(&str, Vec<SuiteDescriptor>)>,
    run_config: RunConfig,
    output_dir: &str,
    options: RunOptions,
) -> Result<()> {
    let sink: std::sync::Arc<dyn EventSink> = if options.json {
        std::sync::Arc::new(JsonEventSink::stdout())
    } else {
        println!();
        println!("{}", ProgressEventSink::header_line());
        std::sync::Arc::new(ProgressEventSink::new())
    };
    set_global_sink(sink);

    let mut reports = Vec::new();
    for (_crate_name, suites) in registries {
        for suite in suites {
            reports.push(
                (suite.build)()
                    .await?
                    .run_box(run_config.clone(), output_dir)
                    .await?,
            );
        }
    }
    let _ = reports;
    Ok(())
}
