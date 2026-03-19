use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub(super) struct EvalCrate {
    pub package_name: String,
    pub crate_ident: String,
    pub manifest_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

#[derive(Debug, Deserialize)]
struct MetadataTarget {
    kind: Vec<String>,
    crate_types: Vec<String>,
}

pub(super) fn discover_eval_crates(workspace_root: &Path) -> Result<Vec<EvalCrate>> {
    let metadata = cargo_metadata(workspace_root)?;
    let workspace_members = metadata
        .workspace_members
        .into_iter()
        .collect::<BTreeSet<_>>();

    let mut crates = BTreeMap::<String, EvalCrate>::new();

    for package in metadata.packages {
        if !workspace_members.contains(&package.id) {
            continue;
        }

        let manifest_dir = package
            .manifest_path
            .parent()
            .context("package manifest without parent directory")?
            .to_path_buf();

        if !manifest_dir.join("evals").exists() {
            continue;
        }

        if !package.targets.iter().any(is_library_target) {
            bail!(
                "workspace package {:?} defines evals/ but has no library target; add a [lib] target so cargo-evals can import __evals_registry()",
                package.name
            );
        }

        crates
            .entry(package.name.clone())
            .or_insert_with(|| EvalCrate {
                crate_ident: package.name.replace('-', "_"),
                package_name: package.name,
                manifest_dir,
            });
    }

    Ok(crates.into_values().collect())
}

fn is_library_target(target: &MetadataTarget) -> bool {
    target
        .kind
        .iter()
        .chain(target.crate_types.iter())
        .any(|kind| matches!(kind.as_str(), "lib" | "rlib" | "cdylib" | "dylib"))
}

fn cargo_metadata(workspace_root: &Path) -> Result<Metadata> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .current_dir(workspace_root)
        .output()
        .context("run cargo metadata for eval discovery")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed during eval discovery: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout).context("parse cargo metadata for eval discovery")
}
