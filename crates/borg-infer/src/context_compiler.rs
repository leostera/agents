use std::fs;
use std::io::{Cursor, Read};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::{
    GenerationFinishReason, GenerationId, GenerationParams, GenerationReport, InferError, Result,
};

const DEFAULT_CONTEXT_TOKENS: u32 = 2048;
const PRECOMPILED_MAGIC: &[u8; 8] = b"BORGPC01";

#[derive(Debug, Clone, Copy)]
pub struct CompileParams {
    pub n_ctx: u32,
}

impl Default for CompileParams {
    fn default() -> Self {
        Self {
            n_ctx: DEFAULT_CONTEXT_TOKENS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextCompiler {
    static_text: String,
    params: CompileParams,
    debug: bool,
}

impl ContextCompiler {
    pub fn builder() -> ContextCompilerBuilder {
        ContextCompilerBuilder::new()
    }

    pub fn compile(self, model_path: &Path) -> Result<CompiledContext> {
        let mut backend = LlamaBackend::init().map_err(|error| {
            InferError::Engine(format!("failed to init llama backend: {error}"))
        })?;
        if !self.debug {
            backend.void_logs();
        }

        if !model_path.is_file() {
            return Err(InferError::InvalidModelPath {
                path: model_path.display().to_string(),
            });
        }

        let model = LlamaModel::load_from_file(&backend, model_path, &LlamaModelParams::default())
            .map_err(|error| InferError::Engine(format!("failed to load GGUF model: {error}")))?;

        let static_tokens = model
            .str_to_token(&self.static_text, AddBos::Always)
            .map_err(|error| {
                InferError::Engine(format!("static context tokenization failed: {error}"))
            })?;

        let mut ctx = model
            .new_context(
                &backend,
                build_context_params(&static_tokens, self.params.n_ctx, 0),
            )
            .map_err(|error| {
                InferError::Engine(format!("failed to create llama context: {error}"))
            })?;

        prefill_prompt(&mut ctx, &static_tokens, 0)?;

        let state_size = ctx.get_state_size();
        let mut state_data = vec![0_u8; state_size];
        let copied = unsafe { ctx.copy_state_data(state_data.as_mut_ptr()) };
        if copied == 0 {
            return Err(InferError::Engine(
                "failed to copy compiled context state".to_string(),
            ));
        }
        state_data.truncate(copied);
        drop(ctx);

        let static_token_count = i32::try_from(static_tokens.len())
            .map_err(|_| InferError::Engine("compiled context too large".to_string()))?;

        Ok(CompiledContext {
            backend,
            model,
            state_data,
            static_token_count,
            context_tokens: self.params.n_ctx.max(DEFAULT_CONTEXT_TOKENS),
            next_generation_id: AtomicU64::new(1),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ContextCompilerBuilder {
    static_text: String,
    params: CompileParams,
    debug: bool,
}

impl ContextCompilerBuilder {
    pub fn new() -> Self {
        Self {
            static_text: String::new(),
            params: CompileParams::default(),
            debug: false,
        }
    }

    pub fn static_text(mut self, text: impl Into<String>) -> Self {
        self.static_text = text.into();
        self
    }

    pub fn params(mut self, params: CompileParams) -> Self {
        self.params = params;
        self
    }

    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn compile(self, model_path: &Path) -> Result<CompiledContext> {
        ContextCompiler {
            static_text: self.static_text,
            params: self.params,
            debug: self.debug,
        }
        .compile(model_path)
    }
}

pub struct CompiledContext {
    backend: LlamaBackend,
    model: LlamaModel,
    state_data: Vec<u8>,
    static_token_count: i32,
    context_tokens: u32,
    next_generation_id: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct CompiledGeneration {
    pub output: String,
    pub report: GenerationReport,
}

impl CompiledContext {
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let mut out = Vec::with_capacity(self.state_data.len().saturating_add(32));
        out.extend_from_slice(PRECOMPILED_MAGIC);
        out.extend_from_slice(&self.context_tokens.to_le_bytes());
        out.extend_from_slice(&self.static_token_count.to_le_bytes());
        out.extend_from_slice(&(self.state_data.len() as u64).to_le_bytes());
        out.extend_from_slice(&self.state_data);
        fs::write(path, out).map_err(|error| {
            InferError::Engine(format!(
                "failed to write precompiled context file `{}`: {error}",
                path.display()
            ))
        })?;
        Ok(())
    }

    pub fn load_from_file(model_path: &Path, precompiled_path: &Path, debug: bool) -> Result<Self> {
        if !model_path.is_file() {
            return Err(InferError::InvalidModelPath {
                path: model_path.display().to_string(),
            });
        }

        let bytes = fs::read(precompiled_path).map_err(|error| {
            InferError::Engine(format!(
                "failed to read precompiled context file `{}`: {error}",
                precompiled_path.display()
            ))
        })?;
        let mut reader = Cursor::new(bytes);

        let mut magic = [0_u8; 8];
        reader.read_exact(&mut magic).map_err(|error| {
            InferError::Engine(format!("invalid precompiled file header: {error}"))
        })?;
        if &magic != PRECOMPILED_MAGIC {
            return Err(InferError::Engine(format!(
                "invalid precompiled file `{}`: magic mismatch",
                precompiled_path.display()
            )));
        }

        let context_tokens = read_u32(&mut reader)?;
        let static_token_count = read_i32(&mut reader)?;
        let state_len = read_u64(&mut reader)? as usize;
        let mut state_data = vec![0_u8; state_len];
        reader.read_exact(&mut state_data).map_err(|error| {
            InferError::Engine(format!(
                "invalid precompiled file `{}`: truncated state payload: {error}",
                precompiled_path.display()
            ))
        })?;

        if reader.position() != reader.get_ref().len() as u64 {
            return Err(InferError::Engine(format!(
                "invalid precompiled file `{}`: trailing bytes",
                precompiled_path.display()
            )));
        }

        let mut backend = LlamaBackend::init().map_err(|error| {
            InferError::Engine(format!("failed to init llama backend: {error}"))
        })?;
        if !debug {
            backend.void_logs();
        }

        let model = LlamaModel::load_from_file(&backend, model_path, &LlamaModelParams::default())
            .map_err(|error| InferError::Engine(format!("failed to load GGUF model: {error}")))?;

        Ok(Self {
            backend,
            model,
            state_data,
            static_token_count,
            context_tokens: context_tokens.max(DEFAULT_CONTEXT_TOKENS),
            next_generation_id: AtomicU64::new(1),
        })
    }

    pub fn generate(&self, prompt: &str, params: &GenerationParams) -> Result<CompiledGeneration> {
        let generation_started = Instant::now();
        let prompt_tokens = self
            .model
            .str_to_token(prompt, AddBos::Never)
            .map_err(|error| InferError::Engine(format!("prompt tokenization failed: {error}")))?;

        let mut ctx = self
            .model
            .new_context(
                &self.backend,
                build_context_params(&prompt_tokens, self.context_tokens, self.static_token_count),
            )
            .map_err(|error| {
                InferError::Engine(format!("failed to create llama context: {error}"))
            })?;

        let read = unsafe { ctx.set_state_data(&self.state_data) };
        if read == 0 {
            return Err(InferError::Engine(
                "failed to restore compiled context state".to_string(),
            ));
        }

        let mut last_logits_index =
            prefill_prompt(&mut ctx, &prompt_tokens, self.static_token_count)?;

        let mut sampler = build_sampler(params);
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut generated_tokens = 0_u32;

        let mut n_cur = self
            .static_token_count
            .saturating_add(i32::try_from(prompt_tokens.len()).unwrap_or(i32::MAX));

        let mut batch = LlamaBatch::new(512, 1);
        while generated_tokens < params.max_tokens {
            let token = sampler.sample(&ctx, last_logits_index);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                let generation_ms = generation_started.elapsed().as_millis();
                return Ok(CompiledGeneration {
                    output,
                    report: GenerationReport {
                        generation_id: next_generation_id(&self.next_generation_id),
                        prompt_tokens: prompt_tokens.len() as u32,
                        generated_tokens,
                        generation_ms,
                        finish_reason: GenerationFinishReason::EndOfGenerationToken,
                    },
                });
            }

            let piece = self
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|error| InferError::Engine(format!("failed to decode token: {error}")))?;

            if !piece.is_empty() {
                output.push_str(&piece);
            }

            generated_tokens = generated_tokens.saturating_add(1);

            batch.clear();
            batch.add(token, n_cur, &[0], true).map_err(|error| {
                InferError::Engine(format!("failed to add generated token to batch: {error}"))
            })?;
            n_cur = n_cur.saturating_add(1);

            ctx.decode(&mut batch).map_err(|error| {
                InferError::Engine(format!("llama decode failed during generation: {error}"))
            })?;
            // Single-token decode batches always expose logits at slot 0.
            last_logits_index = 0;
        }

        let generation_ms = generation_started.elapsed().as_millis();
        Ok(CompiledGeneration {
            output,
            report: GenerationReport {
                generation_id: next_generation_id(&self.next_generation_id),
                prompt_tokens: prompt_tokens.len() as u32,
                generated_tokens,
                generation_ms,
                finish_reason: GenerationFinishReason::MaxTokens,
            },
        })
    }
}

fn read_u32(reader: &mut Cursor<Vec<u8>>) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes).map_err(|error| {
        InferError::Engine(format!("failed to read u32 from precompiled file: {error}"))
    })?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i32(reader: &mut Cursor<Vec<u8>>) -> Result<i32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes).map_err(|error| {
        InferError::Engine(format!("failed to read i32 from precompiled file: {error}"))
    })?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut Cursor<Vec<u8>>) -> Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes).map_err(|error| {
        InferError::Engine(format!("failed to read u64 from precompiled file: {error}"))
    })?;
    Ok(u64::from_le_bytes(bytes))
}

fn next_generation_id(counter: &AtomicU64) -> GenerationId {
    counter.fetch_add(1, Ordering::Relaxed)
}

fn build_context_params(
    prompt_tokens: &[llama_cpp_2::token::LlamaToken],
    n_ctx: u32,
    n_past: i32,
) -> LlamaContextParams {
    let required_tokens = prompt_tokens
        .len()
        .saturating_add(usize::try_from(n_past.max(0)).unwrap_or(usize::MAX))
        .saturating_add(8);
    let requested = u32::try_from(required_tokens)
        .unwrap_or(u32::MAX)
        .max(n_ctx)
        .max(DEFAULT_CONTEXT_TOKENS);
    let n_ctx = NonZeroU32::new(requested).or_else(|| NonZeroU32::new(DEFAULT_CONTEXT_TOKENS));

    let threads = std::thread::available_parallelism()
        .ok()
        .and_then(|value| i32::try_from(value.get()).ok())
        .unwrap_or(4);

    LlamaContextParams::default()
        .with_n_ctx(n_ctx)
        .with_n_threads(threads)
        .with_n_threads_batch(threads)
}

fn prefill_prompt(
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    prompt_tokens: &[llama_cpp_2::token::LlamaToken],
    n_past: i32,
) -> Result<i32> {
    if prompt_tokens.is_empty() {
        return Ok(0);
    }

    let n_batch = usize::try_from(ctx.n_batch())
        .ok()
        .filter(|v| *v > 0)
        .unwrap_or(512);
    let mut start = 0_usize;
    let mut last_logits_index = 0_i32;

    while start < prompt_tokens.len() {
        let end = std::cmp::min(start.saturating_add(n_batch), prompt_tokens.len());
        let chunk = &prompt_tokens[start..end];
        let mut batch = LlamaBatch::new(std::cmp::max(chunk.len().saturating_add(1), 512), 1);

        for (local_idx, token) in chunk.iter().copied().enumerate() {
            let global_idx = start.saturating_add(local_idx);
            let pos = n_past
                .checked_add(i32::try_from(global_idx).map_err(|_| {
                    InferError::Engine("prompt too long for llama batch".to_string())
                })?)
                .ok_or_else(|| InferError::Engine("prompt position overflow".to_string()))?;
            let is_last = global_idx == prompt_tokens.len().saturating_sub(1);
            if is_last {
                last_logits_index = i32::try_from(local_idx).unwrap_or(i32::MAX);
            }
            batch.add(token, pos, &[0], is_last).map_err(|error| {
                InferError::Engine(format!("failed to add prompt token to batch: {error}"))
            })?;
        }

        ctx.decode(&mut batch).map_err(|error| {
            InferError::Engine(format!("llama decode failed for prompt prefill: {error}"))
        })?;

        start = end;
    }
    Ok(last_logits_index)
}

fn build_sampler(params: &GenerationParams) -> LlamaSampler {
    if params.temperature <= 0.0 {
        return LlamaSampler::chain_simple([LlamaSampler::greedy()]);
    }

    let mut chain = Vec::new();
    if params.top_k > 0 {
        chain.push(LlamaSampler::top_k(params.top_k));
    }
    if params.top_p > 0.0 {
        chain.push(LlamaSampler::top_p(params.top_p.min(1.0), 1));
    }
    chain.push(LlamaSampler::temp(params.temperature));
    chain.push(LlamaSampler::dist(params.seed));
    LlamaSampler::chain_simple(chain)
}
