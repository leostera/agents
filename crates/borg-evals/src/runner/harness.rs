use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::quote;
use syn::parse_quote;

use crate::runner::config::EvalsFile;
use crate::runner::discovery::EvalCrate;

const HARNESS_ROOT: &str = "target/cargo-evals/harness";

pub(super) struct GeneratedHarness {
    root: PathBuf,
    manifest_path: PathBuf,
}

impl GeneratedHarness {
    pub(super) fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub(super) fn binary_path(&self) -> PathBuf {
        let binary = if std::env::consts::EXE_EXTENSION.is_empty() {
            "cargo-evals-harness".to_string()
        } else {
            format!("cargo-evals-harness.{}", std::env::consts::EXE_EXTENSION)
        };
        self.root.join("target").join("debug").join(binary)
    }
}

pub(super) fn generate_harness(
    workspace_root: &Path,
    config: &EvalsFile,
    crates: &[EvalCrate],
) -> Result<GeneratedHarness> {
    let root = workspace_root.join(HARNESS_ROOT);
    std::fs::create_dir_all(root.join("src"))
        .with_context(|| format!("create {}", root.display()))?;

    let manifest_path = root.join("Cargo.toml");
    let main_path = root.join("src/main.rs");

    std::fs::write(&manifest_path, render_manifest(workspace_root, crates))
        .with_context(|| format!("write {}", manifest_path.display()))?;
    std::fs::write(&main_path, render_main(config, crates)?)
        .with_context(|| format!("write {}", main_path.display()))?;

    Ok(GeneratedHarness {
        root,
        manifest_path,
    })
}

fn render_manifest(workspace_root: &Path, crates: &[EvalCrate]) -> String {
    let mut out = String::new();
    out.push_str("[package]\n");
    out.push_str("name = \"cargo-evals-harness\"\n");
    out.push_str("version = \"0.1.0\"\n");
    out.push_str("edition = \"2024\"\n\n");
    out.push_str("[workspace]\n\n");
    out.push_str("[dependencies]\n");
    out.push_str("anyhow = \"1\"\n");
    out.push_str("tokio = { version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }\n");
    out.push_str(&format!(
        "borg-evals = {{ path = {:?} }}\n",
        workspace_root
            .join("crates/borg-evals")
            .display()
            .to_string()
    ));

    for krate in crates
        .iter()
        .map(|krate| &krate.package_name)
        .collect::<BTreeSet<_>>()
    {
        let crate_path = workspace_root.join("crates").join(krate);
        out.push_str(&format!(
            "{} = {{ path = {:?} }}\n",
            krate,
            crate_path.display().to_string()
        ));
    }

    out
}

fn render_main(config: &EvalsFile, crates: &[EvalCrate]) -> Result<String> {
    let crates_imports = crates.iter().map(|krate| {
        let crate_ident = syn::Ident::new(&krate.crate_ident, proc_macro2::Span::call_site());
        quote!(use #crate_ident as _;)
    });

    let registries = crates.iter().map(|krate| {
        let crate_ident = syn::Ident::new(&krate.crate_ident, proc_macro2::Span::call_site());
        let package_name = &krate.package_name;
        quote! {
            (#package_name, #crate_ident::__evals_registry())
        }
    });

    let targets = config.evals.targets.iter().map(|target| {
        let label = target.label.as_deref().unwrap_or("");
        let provider = &target.provider;
        let model = &target.model;
        let concurrency = target.concurrency.unwrap_or(1);
        quote! {
            ::borg_evals::ExecutionTarget::new(#label, #provider, #model)
                .with_max_in_flight(#concurrency)
        }
    });

    let trials = config.evals.trials;
    let output_dir = &config.evals.output_dir;

    let file: syn::File = parse_quote! {
        use anyhow::Result;

        #(#crates_imports)*

        #[tokio::main]
        async fn main() -> Result<()> {
            let command = std::env::args().nth(1).unwrap_or_else(|| "run".to_string());
            let json = std::env::args().any(|arg| arg == "--json");
            let registries = vec![#(#registries),*];
            let run_config = ::borg_evals::RunConfig::new(vec![#(#targets),*]).with_trials(#trials);

            match command.as_str() {
                "list" => {
                    ::borg_evals::runner::list_discovered(&registries, &run_config, json);
                }
                "run" => {
                    ::borg_evals::runner::run_discovered(
                        registries,
                        run_config,
                        #output_dir,
                        ::borg_evals::runner::RunOptions { json },
                    ).await?;
                }
                other => {
                    anyhow::bail!("unsupported harness command: {}", other);
                }
            }

            Ok(())
        }
    };

    Ok(prettyplease::unparse(&file))
}
