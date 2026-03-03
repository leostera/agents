use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use borg_infer::{
    CompileParams, CompiledContext, ContextCompiler, EmbeddedInferenceRuntime, GenerationParams,
    InferenceRuntime, LlamaCppEngine, hardcoded_models,
};
use clap::Subcommand;
use serde_json::{Value, json};

use crate::app::BorgCliApp;

const EVENT_CONTEXT: &str = "https://borg.dev/schemas/infer-event/v1";

#[derive(Subcommand, Debug)]
pub enum InferCommand {
    #[command(about = "List hardcoded local GGUF model entries")]
    Models,
    #[command(
        about = "Compile static context and persist it to disk",
        visible_alias = "precopmile"
    )]
    Precompile {
        #[arg(help = "Path to GGUF model file")]
        gguf_path: PathBuf,
        #[arg(help = "Static text to prefill and compile")]
        text: String,
        #[arg(long, help = "Output precompiled context file path")]
        output: PathBuf,
        #[arg(long, default_value_t = false, help = "Enable verbose llama.cpp logs")]
        debug: bool,
    },
    #[command(about = "Run repeated local inference and emit NDJSON JSON-LD bench events")]
    Bench {
        #[arg(
            long,
            help = "Path to GGUF model file (optional if exactly one .gguf exists in cwd)"
        )]
        gguf: Option<PathBuf>,
        #[arg(long, default_value_t = 100, help = "Number of benchmark runs")]
        runs: u32,
        #[arg(long, help = "Prompt text to run on each benchmark execution")]
        prompt: String,
        #[arg(
            long,
            help = "Compile this static prefix once, then benchmark generation against only prompt",
            conflicts_with = "load"
        )]
        compiled_prefix: Option<String>,
        #[arg(
            long,
            value_name = "PRECOMP_FILE",
            help = "Load compiled context from file and benchmark from it",
            conflicts_with = "compiled_prefix"
        )]
        load: Option<PathBuf>,
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
        #[arg(long, default_value_t = false, help = "Enable verbose llama.cpp logs")]
        debug: bool,
    },
    #[command(about = "Run local embedded inference and emit NDJSON JSON-LD events")]
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
        #[arg(
            long,
            help = "Compile this static prefix once, then run generation against only the input prompt"
        )]
        compiled_prefix: Option<String>,
        #[arg(
            long,
            value_name = "PRECOMP_FILE",
            help = "Load compiled context from file and run generation from it",
            conflicts_with = "compiled_prefix"
        )]
        load: Option<PathBuf>,
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
        #[arg(long, default_value_t = false, help = "Enable verbose llama.cpp logs")]
        debug: bool,
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
        InferCommand::Precompile {
            gguf_path,
            text,
            output,
            debug,
        } => {
            let gguf_path = resolve_gguf_path(&gguf_path)?;
            emit_event(
                "borg.infer.precompile.started",
                json!({
                    "gguf_path": gguf_path,
                    "output": output,
                    "text_chars": text.chars().count(),
                    "debug": debug,
                }),
            )?;

            let compile_started = Instant::now();
            let compiled = ContextCompiler::builder()
                .static_text(text)
                .params(CompileParams { n_ctx: 512 })
                .debug(debug)
                .compile(&gguf_path)?;
            let compile_ms = compile_started.elapsed().as_millis();

            let save_started = Instant::now();
            compiled.save_to_file(&output)?;
            let save_ms = save_started.elapsed().as_millis();

            let file_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

            emit_event(
                "borg.infer.precompile.completed",
                json!({
                    "gguf_path": gguf_path,
                    "output": output,
                    "compile_ms": compile_ms,
                    "save_ms": save_ms,
                    "file_bytes": file_size,
                }),
            )?;
            Ok(())
        }
        InferCommand::Bench {
            gguf,
            runs,
            prompt,
            compiled_prefix,
            load,
            max_tokens,
            temperature,
            top_p,
            top_k,
            seed,
            debug,
        } => {
            if runs == 0 {
                bail!("--runs must be greater than zero");
            }
            let gguf_path = resolve_bench_gguf_path(gguf)?;
            let params = GenerationParams {
                max_tokens,
                temperature,
                top_p,
                top_k,
                seed,
            };
            run_bench(
                &gguf_path,
                runs,
                &prompt,
                compiled_prefix,
                load,
                &params,
                debug,
            )?;
            Ok(())
        }
        InferCommand::Run {
            gguf_path,
            input,
            model_id,
            executions,
            initial_prefix,
            compiled_prefix,
            load,
            max_tokens,
            temperature,
            top_p,
            top_k,
            seed,
            debug,
        } => {
            let gguf_path = resolve_gguf_path(&gguf_path)?;
            let model_id = model_id.trim().to_string();
            let params = GenerationParams {
                max_tokens,
                temperature,
                top_p,
                top_k,
                seed,
            };

            emit_event(
                "borg.infer.run.started",
                json!({
                    "model_id": model_id,
                    "gguf_path": gguf_path,
                    "executions": executions,
                    "compiled": compiled_prefix.is_some(),
                    "loaded_precompiled": load.is_some(),
                    "debug": debug,
                }),
            )?;

            if let Some(precompiled_path) = load {
                run_loaded(
                    &precompiled_path,
                    &gguf_path,
                    &input,
                    executions,
                    &params,
                    &model_id,
                    debug,
                )?;
                return Ok(());
            }

            if let Some(compiled_prefix) = compiled_prefix {
                run_compiled(
                    &gguf_path,
                    &input,
                    executions,
                    &params,
                    compiled_prefix,
                    &model_id,
                    debug,
                )?;
                return Ok(());
            }

            run_standard(
                &gguf_path,
                &input,
                executions,
                &params,
                &initial_prefix,
                &model_id,
                debug,
            )?;
            Ok(())
        }
    }
}

fn run_bench(
    gguf_path: &Path,
    runs: u32,
    prompt: &str,
    compiled_prefix: Option<String>,
    load: Option<PathBuf>,
    params: &GenerationParams,
    debug: bool,
) -> Result<()> {
    emit_event(
        "borg.infer.bench.started",
        json!({
            "gguf_path": gguf_path,
            "runs": runs,
            "compiled": compiled_prefix.is_some(),
            "loaded_precompiled": load.is_some(),
            "debug": debug,
        }),
    )?;

    let prep_started = Instant::now();
    enum BenchMode {
        Standard(EmbeddedInferenceRuntime<LlamaCppEngine>),
        Compiled(CompiledContext),
    }

    let mode = if let Some(path) = load {
        emit_event(
            "borg.infer.precompiled.load.started",
            json!({ "gguf_path": gguf_path, "precompiled_path": path }),
        )?;
        let compiled = CompiledContext::load_from_file(gguf_path, &path, debug)?;
        emit_event(
            "borg.infer.precompiled.load.completed",
            json!({ "precompiled_path": path }),
        )?;
        BenchMode::Compiled(compiled)
    } else if let Some(prefix) = compiled_prefix {
        emit_event(
            "borg.infer.compile.started",
            json!({
                "gguf_path": gguf_path,
                "compiled_prefix_chars": prefix.chars().count(),
            }),
        )?;
        let compiled = ContextCompiler::builder()
            .static_text(prefix)
            .params(CompileParams { n_ctx: 512 })
            .debug(debug)
            .compile(gguf_path)?;
        emit_event("borg.infer.compile.completed", json!({}))?;
        BenchMode::Compiled(compiled)
    } else {
        let engine = LlamaCppEngine::new_with_debug(debug)?;
        let runtime = EmbeddedInferenceRuntime::new(engine);
        runtime.load("local/bench", gguf_path)?;
        BenchMode::Standard(runtime)
    };
    let prep_ms = prep_started.elapsed().as_millis();

    let bench_started = Instant::now();
    let mut total_prompt_tokens = 0_u64;
    let mut total_generated_tokens = 0_u64;
    let mut total_generation_ms = 0_u128;
    let mut finish_reasons = Vec::with_capacity(runs as usize);

    for run_index in 1..=runs {
        emit_event("borg.infer.bench.run.started", json!({ "run": run_index }))?;

        match &mode {
            BenchMode::Standard(runtime) => {
                let report = runtime.generate("local/bench", prompt, params, &mut |_chunk| {})?;
                total_prompt_tokens =
                    total_prompt_tokens.saturating_add(u64::from(report.prompt_tokens));
                total_generated_tokens =
                    total_generated_tokens.saturating_add(u64::from(report.generated_tokens));
                total_generation_ms = total_generation_ms.saturating_add(report.generation_ms);
                finish_reasons.push(report.finish_reason.as_str().to_string());
                emit_event(
                    "borg.infer.bench.run.completed",
                    json!({
                        "run": run_index,
                        "generation_ms": report.generation_ms,
                        "prompt_tokens": report.prompt_tokens,
                        "generated_tokens": report.generated_tokens,
                        "finish_reason": report.finish_reason.as_str(),
                    }),
                )?;
            }
            BenchMode::Compiled(compiled) => {
                let generated = compiled.generate(prompt, params)?;
                total_prompt_tokens =
                    total_prompt_tokens.saturating_add(u64::from(generated.report.prompt_tokens));
                total_generated_tokens = total_generated_tokens
                    .saturating_add(u64::from(generated.report.generated_tokens));
                total_generation_ms =
                    total_generation_ms.saturating_add(generated.report.generation_ms);
                finish_reasons.push(generated.report.finish_reason.as_str().to_string());
                emit_event(
                    "borg.infer.bench.run.completed",
                    json!({
                        "run": run_index,
                        "generation_ms": generated.report.generation_ms,
                        "prompt_tokens": generated.report.prompt_tokens,
                        "generated_tokens": generated.report.generated_tokens,
                        "finish_reason": generated.report.finish_reason.as_str(),
                    }),
                )?;
            }
        }
    }

    let wall_ms = bench_started.elapsed().as_millis();
    let avg_generation_ms = total_generation_ms as f64 / f64::from(runs);
    let tokens_per_second = if total_generation_ms == 0 {
        total_generated_tokens as f64
    } else {
        (total_generated_tokens as f64 * 1000.0) / total_generation_ms as f64
    };

    emit_event(
        "borg.infer.bench.completed",
        json!({
            "gguf_path": gguf_path,
            "runs_requested": runs,
            "runs_completed": runs,
            "prep_ms": prep_ms,
            "wall_ms": wall_ms,
            "total_generation_ms": total_generation_ms,
            "avg_generation_ms": avg_generation_ms,
            "prompt_tokens": total_prompt_tokens,
            "generated_tokens": total_generated_tokens,
            "tokens_per_second": tokens_per_second,
            "finish_reasons": finish_reasons,
        }),
    )?;

    Ok(())
}

fn run_standard(
    gguf_path: &Path,
    input: &str,
    executions: u32,
    params: &GenerationParams,
    initial_prefix: &str,
    model_id: &str,
    debug: bool,
) -> Result<()> {
    let engine = LlamaCppEngine::new_with_debug(debug)?;
    let runtime = EmbeddedInferenceRuntime::new(engine);

    emit_event(
        "borg.infer.model.load.started",
        json!({ "model_id": model_id, "gguf_path": gguf_path }),
    )?;
    let load = runtime.load(model_id, gguf_path)?;
    emit_event(
        "borg.infer.model.load.completed",
        json!({
            "model_id": load.model_id,
            "gguf_path": load.model_path,
            "model_load_ms": load.model_load_ms,
            "model_reloaded": load.reloaded,
        }),
    )?;

    let prompt = compose_prompt(initial_prefix, input);

    let mut outputs = Vec::with_capacity(executions as usize);
    let mut generation_ids = Vec::with_capacity(executions as usize);
    let mut finish_reasons = Vec::with_capacity(executions as usize);
    let mut prompt_tokens = 0_u64;
    let mut generated_tokens = 0_u64;
    let mut generation_ms = 0_u128;

    for execution in 1..=executions {
        emit_event(
            "borg.infer.execution.started",
            json!({ "execution": execution, "mode": "standard" }),
        )?;

        let mut output = String::new();
        let report = runtime.generate(model_id, &prompt, params, &mut |chunk| {
            output.push_str(chunk);
        })?;

        prompt_tokens = prompt_tokens.saturating_add(u64::from(report.prompt_tokens));
        generated_tokens = generated_tokens.saturating_add(u64::from(report.generated_tokens));
        generation_ms = generation_ms.saturating_add(report.generation_ms);
        generation_ids.push(report.generation_id);
        finish_reasons.push(report.finish_reason.as_str().to_string());

        emit_event(
            "borg.infer.execution.completed",
            json!({
                "execution": execution,
                "generation_id": report.generation_id,
                "prompt_tokens": report.prompt_tokens,
                "generated_tokens": report.generated_tokens,
                "generation_ms": report.generation_ms,
                "finish_reason": report.finish_reason.as_str(),
                "output": output,
            }),
        )?;

        outputs.push(output);
    }

    let tokens_per_second = if generation_ms == 0 {
        generated_tokens as f32
    } else {
        (generated_tokens as f32 * 1000.0) / generation_ms as f32
    };

    emit_event(
        "borg.infer.run.completed",
        json!({
            "mode": "standard",
            "model_id": model_id,
            "gguf_path": gguf_path,
            "output": outputs.join(""),
            "outputs": outputs,
            "summary": {
                "executions_requested": executions,
                "executions_completed": executions,
                "generation_ids": generation_ids,
                "prompt_tokens": prompt_tokens,
                "generated_tokens": generated_tokens,
                "generation_ms": generation_ms,
                "tokens_per_second": tokens_per_second,
                "finish_reasons": finish_reasons,
            }
        }),
    )?;

    Ok(())
}

fn run_compiled(
    gguf_path: &Path,
    input: &str,
    executions: u32,
    params: &GenerationParams,
    compiled_prefix: String,
    model_id: &str,
    debug: bool,
) -> Result<()> {
    emit_event(
        "borg.infer.compile.started",
        json!({
            "model_id": model_id,
            "gguf_path": gguf_path,
            "compiled_prefix_chars": compiled_prefix.chars().count(),
        }),
    )?;

    let compile_started = Instant::now();
    let compiled = ContextCompiler::builder()
        .static_text(compiled_prefix)
        .params(CompileParams { n_ctx: 512 })
        .debug(debug)
        .compile(gguf_path)?;
    let compile_ms = compile_started.elapsed().as_millis();

    emit_event(
        "borg.infer.compile.completed",
        json!({ "model_id": model_id, "compile_ms": compile_ms }),
    )?;

    let mut outputs = Vec::with_capacity(executions as usize);
    let mut generation_ids = Vec::with_capacity(executions as usize);
    let mut finish_reasons = Vec::with_capacity(executions as usize);
    let mut prompt_tokens = 0_u64;
    let mut generated_tokens = 0_u64;

    for execution in 1..=executions {
        emit_event(
            "borg.infer.execution.started",
            json!({ "execution": execution, "mode": "compiled" }),
        )?;

        let generated = compiled.generate(input, params)?;
        prompt_tokens = prompt_tokens.saturating_add(u64::from(generated.report.prompt_tokens));
        generated_tokens =
            generated_tokens.saturating_add(u64::from(generated.report.generated_tokens));
        generation_ids.push(generated.report.generation_id);
        finish_reasons.push(generated.report.finish_reason.as_str().to_string());

        emit_event(
            "borg.infer.execution.completed",
            json!({
                "execution": execution,
                "generation_id": generated.report.generation_id,
                "prompt_tokens": generated.report.prompt_tokens,
                "generated_tokens": generated.report.generated_tokens,
                "finish_reason": generated.report.finish_reason.as_str(),
                "output": generated.output,
            }),
        )?;

        outputs.push(generated.output);
    }

    emit_event(
        "borg.infer.run.completed",
        json!({
            "mode": "compiled",
            "model_id": model_id,
            "gguf_path": gguf_path,
            "output": outputs.join(""),
            "outputs": outputs,
            "summary": {
                "executions_requested": executions,
                "executions_completed": executions,
                "compile_ms": compile_ms,
                "generation_ids": generation_ids,
                "prompt_tokens": prompt_tokens,
                "generated_tokens": generated_tokens,
                "finish_reasons": finish_reasons,
            }
        }),
    )?;

    Ok(())
}

fn run_loaded(
    precompiled_path: &Path,
    gguf_path: &Path,
    input: &str,
    executions: u32,
    params: &GenerationParams,
    model_id: &str,
    debug: bool,
) -> Result<()> {
    emit_event(
        "borg.infer.precompiled.load.started",
        json!({
            "model_id": model_id,
            "gguf_path": gguf_path,
            "precompiled_path": precompiled_path,
        }),
    )?;

    let load_started = Instant::now();
    let compiled = CompiledContext::load_from_file(gguf_path, precompiled_path, debug)?;
    let load_ms = load_started.elapsed().as_millis();

    emit_event(
        "borg.infer.precompiled.load.completed",
        json!({
            "model_id": model_id,
            "precompiled_path": precompiled_path,
            "load_ms": load_ms,
        }),
    )?;

    let mut outputs = Vec::with_capacity(executions as usize);
    let mut generation_ids = Vec::with_capacity(executions as usize);
    let mut finish_reasons = Vec::with_capacity(executions as usize);
    let mut prompt_tokens = 0_u64;
    let mut generated_tokens = 0_u64;

    for execution in 1..=executions {
        emit_event(
            "borg.infer.execution.started",
            json!({ "execution": execution, "mode": "loaded" }),
        )?;

        let generated = compiled.generate(input, params)?;
        prompt_tokens = prompt_tokens.saturating_add(u64::from(generated.report.prompt_tokens));
        generated_tokens =
            generated_tokens.saturating_add(u64::from(generated.report.generated_tokens));
        generation_ids.push(generated.report.generation_id);
        finish_reasons.push(generated.report.finish_reason.as_str().to_string());

        emit_event(
            "borg.infer.execution.completed",
            json!({
                "execution": execution,
                "generation_id": generated.report.generation_id,
                "prompt_tokens": generated.report.prompt_tokens,
                "generated_tokens": generated.report.generated_tokens,
                "finish_reason": generated.report.finish_reason.as_str(),
                "output": generated.output,
            }),
        )?;

        outputs.push(generated.output);
    }

    emit_event(
        "borg.infer.run.completed",
        json!({
            "mode": "loaded",
            "model_id": model_id,
            "gguf_path": gguf_path,
            "precompiled_path": precompiled_path,
            "output": outputs.join(""),
            "outputs": outputs,
            "summary": {
                "executions_requested": executions,
                "executions_completed": executions,
                "precompiled_load_ms": load_ms,
                "generation_ids": generation_ids,
                "prompt_tokens": prompt_tokens,
                "generated_tokens": generated_tokens,
                "finish_reasons": finish_reasons,
            }
        }),
    )?;

    Ok(())
}

fn emit_event(event_type: &str, data: Value) -> Result<()> {
    let event = json!({
        "@context": EVENT_CONTEXT,
        "@type": event_type,
        "ts_ms": now_ms(),
        "data": data,
    });
    println!("{}", serde_json::to_string(&event)?);
    io::stdout().flush()?;
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn compose_prompt(initial_prefix: &str, input: &str) -> String {
    let prefix = initial_prefix.trim();
    if prefix.is_empty() {
        return input.to_string();
    }
    format!("{prefix}\n{input}")
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

fn resolve_bench_gguf_path(path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = path {
        return resolve_gguf_path(&path);
    }

    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&cwd)
        .with_context(|| format!("failed to read directory `{}`", cwd.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        {
            candidates.push(path);
        }
    }

    match candidates.len() {
        0 => bail!("no .gguf file found in current directory; pass --gguf <path>"),
        1 => Ok(candidates.remove(0)),
        _ => {
            bail!("multiple .gguf files found in current directory; pass --gguf <path> explicitly")
        }
    }
}
