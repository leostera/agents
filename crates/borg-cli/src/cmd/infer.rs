use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use borg_infer::{
    ConfiguredRuntime, GenerationParams, InitialState, LlamaCppEngine, RunSpec, RuntimeConfig,
    hardcoded_models,
};
use clap::Subcommand;
use serde_json::json;

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum InferCommand {
    #[command(about = "List hardcoded local GGUF model entries")]
    Models,
    #[command(about = "Run local embedded inference and return JSON")]
    Run {
        #[arg(help = "Path to GGUF model file")]
        gguf_path: PathBuf,
        #[arg(help = "Input text to generate from")]
        input: String,
        #[arg(long, default_value = "local/run", help = "Runtime model id label")]
        model_id: String,
        #[arg(long, default_value_t = 1, help = "Number of executions to run")]
        executions: u32,
        #[arg(long, default_value = "", help = "Initial prompt prefix/state")]
        initial_prefix: String,
        #[arg(long, default_value_t = 128, help = "Maximum generated tokens")]
        max_tokens: u32,
        #[arg(long, default_value_t = 0.8, help = "Sampling temperature")]
        temperature: f32,
        #[arg(long, default_value_t = 0.95, help = "Top-p nucleus sampling")]
        top_p: f32,
        #[arg(long, default_value_t = 40, help = "Top-k sampling")]
        top_k: i32,
        #[arg(long, default_value_t = 1234, help = "RNG seed")]
        seed: u32,
    },
}

pub async fn run(_app: &BorgCliApp, cmd: InferCommand) -> Result<()> {
    match cmd {
        InferCommand::Models => {
            let items = hardcoded_models()
                .iter()
                .map(|entry| {
                    json!({
                        "model_id": entry.model_id,
                        "gguf_path": entry.gguf_path,
                    })
                })
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::to_string(&json!({ "ok": true, "entity": "models", "items": items }))?
            );
            Ok(())
        }
        InferCommand::Run {
            gguf_path,
            input,
            model_id,
            executions,
            initial_prefix,
            max_tokens,
            temperature,
            top_p,
            top_k,
            seed,
        } => {
            let gguf_path = resolve_gguf_path(&gguf_path)?;
            let engine = LlamaCppEngine::new()?;
            let config = RuntimeConfig {
                model_id: model_id.trim().to_string(),
                gguf_path,
                initial_state: InitialState {
                    prompt_prefix: initial_prefix,
                },
                default_params: GenerationParams::default(),
                executions: 1,
            };
            let runtime = ConfiguredRuntime::new(engine, config);
            let run_spec = RunSpec::builder(input)
                .executions(executions)
                .max_tokens(max_tokens)
                .temperature(temperature)
                .top_p(top_p)
                .top_k(top_k)
                .seed(seed)
                .build();
            let result = runtime.run(run_spec).await?;
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "ok": true,
                    "entity": "infer_run",
                    "result": result,
                }))?
            );
            Ok(())
        }
    }
}

fn resolve_gguf_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve current working directory")?
            .join(path)
    };

    if !absolute.is_file() {
        bail!(
            "GGUF model path does not exist or is not a file: {}",
            absolute.display()
        );
    }

    Ok(absolute)
}
