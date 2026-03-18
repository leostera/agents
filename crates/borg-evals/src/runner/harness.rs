use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::quote;
use syn::parse_quote;

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
    crates: &[EvalCrate],
) -> Result<GeneratedHarness> {
    let root = workspace_root.join(HARNESS_ROOT);
    std::fs::create_dir_all(root.join("src"))
        .with_context(|| format!("create {}", root.display()))?;

    let manifest_path = root.join("Cargo.toml");
    let main_path = root.join("src/main.rs");

    std::fs::write(&manifest_path, render_manifest(workspace_root, crates))
        .with_context(|| format!("write {}", manifest_path.display()))?;
    std::fs::write(&main_path, render_main(crates)?)
        .with_context(|| format!("write {}", main_path.display()))?;

    Ok(GeneratedHarness {
        root,
        manifest_path,
    })
}

fn render_manifest(_workspace_root: &Path, crates: &[EvalCrate]) -> String {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let agents_dir = crate_dir
        .parent()
        .expect("borg-evals crate to live under crates/")
        .join("agents");
    let mut out = String::new();
    out.push_str("[package]\n");
    out.push_str("name = \"cargo-evals-harness\"\n");
    out.push_str("version = \"0.1.0\"\n");
    out.push_str("edition = \"2024\"\n\n");
    out.push_str("[workspace]\n\n");
    out.push_str("[dependencies]\n");
    out.push_str("anyhow = \"1\"\n");
    out.push_str("dotenvy = \"0\"\n");
    out.push_str("tokio = { version = \"1\", features = [\"macros\", \"rt-multi-thread\"] }\n");
    out.push_str(&format!(
        "agents = {{ path = {:?} }}\n",
        agents_dir.display().to_string()
    ));

    for krate in crates {
        out.push_str(&format!(
            "{} = {{ path = {:?} }}\n",
            krate.package_name,
            krate.manifest_dir.display().to_string()
        ));
    }

    out
}

fn render_main(crates: &[EvalCrate]) -> Result<String> {
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

    let file: syn::File = parse_quote! {
        use anyhow::Result;

        #(#crates_imports)*

        #[tokio::main]
        async fn main() -> Result<()> {
            let _ = dotenvy::dotenv();
            let mut args = std::env::args().skip(1);
            let command = args.next().unwrap_or_else(|| "run".to_string());
            let mut json = false;
            let mut model = None;
            let mut query = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--json" => json = true,
                    "--model" => {
                        model = Some(
                            args.next().ok_or_else(|| anyhow::anyhow!("missing value for --model"))?
                        );
                    }
                    value if query.is_none() => query = Some(value.to_string()),
                    other => anyhow::bail!("unsupported harness argument: {}", other),
                }
            }
            let registries = vec![#(#registries),*];
            let workspace_root = ::agents::evals::runner::resolve_workspace_root(&std::env::current_dir()?)?;
            let loaded = ::agents::evals::runner::load_workspace_run_config(&workspace_root)?;
            let run_config = loaded.run_config;
            let output_dir = loaded.output_dir;

            match command.as_str() {
                "list" => {
                    ::agents::evals::runner::list_discovered(&registries, &run_config, json);
                }
                "run" => {
                    ::agents::evals::runner::run_discovered(
                        registries,
                        run_config,
                        &output_dir,
                        ::agents::evals::runner::RunOptions {
                            json,
                            filter: ::agents::evals::TargetFilter {
                                query,
                                model,
                            },
                        },
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
