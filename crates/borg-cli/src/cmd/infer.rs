use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use borg_infer::{
    GenerationParams, InferenceRuntime, LlamaCppEngine, LlamaCppRuntime, hardcoded_model_path,
    hardcoded_models,
};
use clap::Subcommand;
use serde_json::json;

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum InferCommand {
    #[command(about = "List hardcoded local GGUF model entries")]
    Models,
    #[command(about = "Run a local embedded inference smoke test")]
    Test {
        #[arg(
            long,
            default_value = "local/default",
            help = "Model id from hardcoded catalog"
        )]
        model: String,
        #[arg(
            long,
            help = "Direct GGUF path override (bypasses hardcoded model path)"
        )]
        gguf: Option<PathBuf>,
        #[arg(long, help = "Prompt to generate from")]
        prompt: String,
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
        InferCommand::Test {
            model,
            gguf,
            prompt,
            max_tokens,
            temperature,
            top_p,
            top_k,
            seed,
        } => {
            let gguf_path = resolve_model_path(&model, gguf.as_deref())?;
            let engine = LlamaCppEngine::new()?;
            let runtime = Arc::new(LlamaCppRuntime::new(engine));
            let load_report = runtime.load(&model, &gguf_path)?;

            let params = GenerationParams {
                max_tokens,
                temperature,
                top_p,
                top_k,
                seed,
            };

            let runtime_for_generation = runtime.clone();
            let model_for_generation = model.clone();
            let prompt_for_generation = prompt.clone();
            let params_for_generation = params.clone();

            let mut generation_task = tokio::task::spawn_blocking(move || -> Result<_> {
                let mut output = std::io::stdout();
                let mut write_error: Option<std::io::Error> = None;

                let report = runtime_for_generation.generate(
                    &model_for_generation,
                    &prompt_for_generation,
                    &params_for_generation,
                    &mut |chunk| {
                        if write_error.is_some() {
                            return;
                        }

                        if let Err(error) = output.write_all(chunk.as_bytes()) {
                            write_error = Some(error);
                            return;
                        }

                        if let Err(error) = output.flush() {
                            write_error = Some(error);
                        }
                    },
                )?;

                if let Some(error) = write_error {
                    return Err(error).context("failed to stream generation output");
                }

                Ok(report)
            });

            let mut cancel_requested = false;
            let generation_report = loop {
                tokio::select! {
                    result = &mut generation_task => {
                        let report = result
                            .context("generation task failed")??;
                        break report;
                    }
                    ctrl = tokio::signal::ctrl_c(), if !cancel_requested => {
                        ctrl?;
                        cancel_requested = true;
                        if let Some(generation_id) = runtime.active_generation_id() {
                            runtime.cancel(generation_id)?;
                            eprintln!("\ncancellation requested for generation {}", generation_id);
                        }
                    }
                }
            };

            println!();
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "ok": true,
                    "entity": "infer_test",
                    "model_id": model,
                    "gguf_path": gguf_path,
                    "prompt_tokens": generation_report.prompt_tokens,
                    "generated_tokens": generation_report.generated_tokens,
                    "generation_ms": generation_report.generation_ms,
                    "tokens_per_second": generation_report.tokens_per_second(),
                    "finish_reason": generation_report.finish_reason.as_str(),
                    "generation_id": generation_report.generation_id,
                    "model_load_ms": load_report.model_load_ms,
                    "reloaded_model": load_report.reloaded,
                }))?
            );
            Ok(())
        }
    }
}

fn resolve_model_path(model_id: &str, gguf_override: Option<&Path>) -> Result<PathBuf> {
    let resolved = match gguf_override {
        Some(path) => path.to_path_buf(),
        None => hardcoded_model_path(model_id).ok_or_else(|| {
            let available = hardcoded_models()
                .iter()
                .map(|entry| format!("{} -> {}", entry.model_id, entry.gguf_path))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::anyhow!(
                "unknown model id `{}` and no --gguf override provided. available hardcoded models: {}",
                model_id,
                available
            )
        })?,
    };

    let absolute = if resolved.is_absolute() {
        resolved
    } else {
        std::env::current_dir()
            .context("failed to resolve current working directory")?
            .join(resolved)
    };

    if !absolute.is_file() {
        bail!(
            "GGUF model path does not exist or is not a file: {}",
            absolute.display()
        );
    }

    Ok(absolute)
}
