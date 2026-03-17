use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::config::EvalsFile;
use crate::discovery::EvalCrate;
use crate::harness;

pub async fn run(config: &EvalsFile, crates: &[EvalCrate]) -> Result<()> {
    let workspace_root = std::env::current_dir().context("resolve workspace root")?;
    let harness = harness::generate(&workspace_root, config, crates)?;

    let status = Command::new("cargo")
        .arg("run")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(harness.manifest_path())
        .arg("--")
        .arg("run")
        .status()
        .context("run generated cargo-evals harness")?;

    if !status.success() {
        bail!("generated cargo-evals harness failed");
    }

    Ok(())
}
