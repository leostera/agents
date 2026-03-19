mod config;
mod discovery;
mod harness;

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::error::EvalError;
use crate::{
    EventSink, JsonEventSink, PlannedSuiteRun, ProgressEventSink, RunConfig, RunEvent,
    SuiteDescriptor, TargetFilter, emit, set_global_sink,
};
use config::EvalsFile;
use discovery::discover_eval_crates;
use harness::generate_harness;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub json: bool,
    pub filter: TargetFilter,
}

pub struct WorkspaceRunConfig {
    pub run_config: RunConfig,
    pub output_dir: String,
}

const EVALS_TOML_TEMPLATE: &str = r#"[evals]
trials = 10
timeout = "30s"
output_dir = ".evals"

[[evals.targets]]
provider = "ollama"
model = "llama3.2:3b"

# Hosted provider examples:
#
# [[evals.targets]]
# provider = "openai"
# model = "gpt-5-mini"
#
# [[evals.targets]]
# provider = "anthropic"
# model = "claude-sonnet-4-5"
#
# [[evals.targets]]
# provider = "openrouter"
# model = "openai/gpt-5-mini"
#
# [[evals.targets]]
# provider = "workers_ai"
# model = "@cf/meta/llama-3.2-3b-instruct"
#
# [[evals.targets]]
# provider = "lm_studio"
# model = "local-model"
#
# Provider-specific configuration:
#
# [provider.openai]
# api_key = "sk-..."
#
# [provider.anthropic]
# api_key = "sk-ant-..."
#
# [provider.openrouter]
# api_key = "sk-or-..."
#
# [provider.workers_ai]
# api_token = "..."
# account_id = "..."
#
# [provider.lm_studio]
# url = "http://127.0.0.1:1234"
"#;

pub fn resolve_workspace_root(start_dir: &Path) -> Result<std::path::PathBuf> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .current_dir(start_dir)
        .output()
        .context("run cargo metadata to resolve workspace root")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed while resolving workspace root: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let metadata = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .context("parse cargo metadata while resolving workspace root")?;
    let workspace_root = metadata
        .get("workspace_root")
        .and_then(serde_json::Value::as_str)
        .context("cargo metadata did not return workspace_root")?;

    Ok(std::path::PathBuf::from(workspace_root))
}

pub fn load_workspace_run_config(workspace_root: &Path) -> Result<WorkspaceRunConfig> {
    let evals_file = EvalsFile::load(workspace_root)?;
    Ok(WorkspaceRunConfig {
        run_config: evals_file.run_config(),
        output_dir: evals_file.output_dir().to_string(),
    })
}

pub fn init_workspace(workspace_root: &Path, force: bool) -> Result<()> {
    let path = workspace_root.join("evals.toml");
    if path.exists() && !force {
        bail!(
            "{} already exists; rerun with --force to overwrite it",
            path.display()
        );
    }

    fs::write(&path, EVALS_TOML_TEMPLATE).with_context(|| format!("write {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

pub fn list_workspace(workspace_root: &Path, options: RunOptions) -> Result<()> {
    let crates = discover_eval_crates(workspace_root)?;
    let harness = generate_harness(workspace_root, &crates)?;

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

pub fn list_models_workspace(workspace_root: &Path) -> Result<()> {
    let evals_file = EvalsFile::load(workspace_root)?;
    for target in evals_file.evals.targets {
        println!(
            "{}",
            target
                .label
                .unwrap_or_else(|| format!("{}/{}", target.provider, target.model))
        );
    }
    Ok(())
}

pub async fn run_workspace(workspace_root: &Path, options: RunOptions) -> Result<()> {
    let crates = discover_eval_crates(workspace_root)?;
    let harness = generate_harness(workspace_root, &crates)?;

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
    if let Some(model) = &options.filter.model {
        command.arg("--model").arg(model);
    }
    if let Some(query) = &options.filter.query {
        command.arg(query);
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
    if run_config.targets.is_empty() {
        return Err(anyhow::Error::new(EvalError::NoTargetsConfigured));
    }
    if registries.iter().all(|(_, suites)| suites.is_empty()) {
        return Err(anyhow::Error::new(EvalError::NoSuitesDiscovered));
    }

    let sink: std::sync::Arc<dyn EventSink> = if options.json {
        std::sync::Arc::new(JsonEventSink::stdout())
    } else {
        std::sync::Arc::new(ProgressEventSink::new())
    };
    set_global_sink(sink);

    let plan = build_discovered_run_plan(&registries, &run_config, &options.filter)?;
    emit(RunEvent::RunPlanned {
        suites: plan
            .iter()
            .map(|suite| PlannedSuiteRun {
                crate_name: suite.crate_name.to_string(),
                suite_id: suite.descriptor.id.to_string(),
                target_labels: suite
                    .targets
                    .iter()
                    .map(|target| target.label.clone())
                    .collect(),
                eval_ids: suite.eval_ids.iter().map(|id| (*id).to_string()).collect(),
            })
            .collect(),
    });

    let mut reports = Vec::new();
    for planned_suite in plan {
        reports.push(
            (planned_suite.descriptor.build)()
                .await?
                .run_box(
                    RunConfig::new(planned_suite.targets)
                        .with_trials(run_config.trials)
                        .with_optional_timeout(run_config.timeout)
                        .with_provider_configs(run_config.provider.clone()),
                    output_dir,
                    options.filter.clone(),
                )
                .await?,
        );
    }

    let _ = reports;
    Ok(())
}

#[derive(Clone, Debug)]
struct DiscoveredSuitePlan<'a> {
    crate_name: &'a str,
    descriptor: SuiteDescriptor,
    targets: Vec<crate::ExecutionTarget>,
    eval_ids: Vec<&'a str>,
}

fn build_discovered_run_plan<'a>(
    registries: &'a [(&'a str, Vec<SuiteDescriptor>)],
    run_config: &RunConfig,
    filter: &TargetFilter,
) -> Result<Vec<DiscoveredSuitePlan<'a>>> {
    let mut plan = Vec::new();

    for (crate_name, suites) in registries {
        for descriptor in suites {
            let mut targets = run_config
                .targets
                .iter()
                .filter(|target| filter.matches_target(target))
                .cloned()
                .collect::<Vec<_>>();

            if targets.is_empty() {
                continue;
            }

            let eval_ids = if let Some(query) = filter.query.as_deref() {
                if descriptor.id.contains(query) {
                    descriptor.eval_ids.to_vec()
                } else {
                    let mut matched_eval_ids = Vec::new();
                    targets.retain(|target| {
                        let matching_evals = descriptor
                            .eval_ids
                            .iter()
                            .copied()
                            .filter(|eval_id| {
                                format!("{}::{}::{}", descriptor.id, target.label, eval_id)
                                    .contains(query)
                            })
                            .collect::<Vec<_>>();
                        for eval_id in &matching_evals {
                            if !matched_eval_ids.contains(eval_id) {
                                matched_eval_ids.push(*eval_id);
                            }
                        }
                        !matching_evals.is_empty()
                    });
                    matched_eval_ids
                }
            } else {
                descriptor.eval_ids.to_vec()
            };

            if targets.is_empty() || eval_ids.is_empty() {
                continue;
            }

            plan.push(DiscoveredSuitePlan {
                crate_name,
                descriptor: *descriptor,
                targets,
                eval_ids,
            });
        }
    }

    if plan.is_empty() {
        if let Some(query) = &filter.query {
            return Err(anyhow::Error::new(EvalError::no_matches_for_query(
                query.clone(),
            )));
        }
        if let Some(model) = &filter.model {
            return Err(anyhow::Error::new(EvalError::no_targets_matched_model(
                model.clone(),
            )));
        }
        return Err(anyhow::Error::new(EvalError::NoSuitesMatchedFilters));
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::init_workspace;

    #[test]
    fn init_workspace_writes_example_config() {
        let dir = TempDir::new().expect("tempdir");

        init_workspace(dir.path(), false).expect("init workspace");

        let content = fs::read_to_string(dir.path().join("evals.toml")).expect("read evals.toml");
        assert!(content.contains("[evals]"));
        assert!(content.contains("[[evals.targets]]"));
        assert!(content.contains("provider = \"ollama\""));
    }

    #[test]
    fn init_workspace_requires_force_to_overwrite() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("evals.toml"), "existing = true").expect("write evals.toml");

        let error = init_workspace(dir.path(), false).expect_err("init to fail");
        assert!(error.to_string().contains("already exists"));

        init_workspace(dir.path(), true).expect("force init workspace");
        let content = fs::read_to_string(dir.path().join("evals.toml")).expect("read evals.toml");
        assert!(content.contains("[[evals.targets]]"));
    }
}
