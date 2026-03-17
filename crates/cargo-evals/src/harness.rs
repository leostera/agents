use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::quote;
use syn::parse_quote;

use crate::config::EvalsFile;
use crate::discovery::EvalCrate;

const HARNESS_ROOT: &str = "target/cargo-evals/harness";

pub struct GeneratedHarness {
    manifest_path: PathBuf,
}

impl GeneratedHarness {
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
}

pub fn generate(
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

    Ok(GeneratedHarness { manifest_path })
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

    let list_lines = crates.iter().map(|krate| {
        let crate_ident = syn::Ident::new(&krate.crate_ident, proc_macro2::Span::call_site());
        let package_name = &krate.package_name;
        quote! {
            for suite in #crate_ident::__evals_registry() {
                println!("crate {}", #package_name);
                println!("suite {}", suite.id);
                for eval_id in suite.eval_ids {
                    println!("  eval {}", eval_id);
                }
            }
        }
    });

    let run_lines = crates.iter().map(|krate| {
        let crate_ident = syn::Ident::new(&krate.crate_ident, proc_macro2::Span::call_site());
        quote! {
            for suite in #crate_ident::__evals_registry() {
                reports.push(
                    (suite.build)()
                        .await?
                        .run_box(run_config.clone(), output_dir)
                        .await?
                );
            }
        }
    });

    let targets = config.evals.targets.iter().map(|target| {
        let label = target.label.as_deref().unwrap_or("");
        let provider = &target.provider;
        let model = &target.model;
        let concurrency = target.concurrency.unwrap_or(1);
        quote! {
            borg_evals::ExecutionTarget::new(#label, #provider, #model)
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

            match command.as_str() {
                "list" => {
                    #(#list_lines)*
                }
                "run" => {
                    let run_config = borg_evals::RunConfig::new(vec![#(#targets),*]).with_trials(#trials);
                    let output_dir = #output_dir;
                    let mut reports = Vec::new();
                    #(#run_lines)*
                    for report in reports {
                        println!("{}", report.summary_table());
                    }
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
