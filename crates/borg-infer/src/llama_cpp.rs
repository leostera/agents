use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::engine::{EngineGenerationOutcome, InferenceEngine};
use crate::runtime::EmbeddedInferenceRuntime;
use crate::types::{GenerationFinishReason, GenerationParams, InferError, Result};

const DEFAULT_CONTEXT_TOKENS: u32 = 2048;

pub struct LlamaCppEngine {
    backend: LlamaBackend,
    model: Option<LlamaModel>,
}

impl LlamaCppEngine {
    pub fn new() -> Result<Self> {
        let backend = LlamaBackend::init().map_err(|error| {
            InferError::Engine(format!("failed to init llama backend: {error}"))
        })?;
        Ok(Self {
            backend,
            model: None,
        })
    }
}

impl InferenceEngine for LlamaCppEngine {
    fn load_model(&mut self, model_path: &Path) -> Result<()> {
        if !model_path.is_file() {
            return Err(InferError::InvalidModelPath {
                path: model_path.display().to_string(),
            });
        }

        let model =
            LlamaModel::load_from_file(&self.backend, model_path, &LlamaModelParams::default())
                .map_err(|error| {
                    InferError::Engine(format!("failed to load GGUF model: {error}"))
                })?;
        self.model = Some(model);
        Ok(())
    }

    fn generate(
        &mut self,
        prompt: &str,
        params: &GenerationParams,
        cancelled: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<EngineGenerationOutcome> {
        let model = self.model.as_ref().ok_or_else(|| {
            InferError::Engine("model must be loaded before generation".to_string())
        })?;

        let prompt_tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|error| InferError::Engine(format!("prompt tokenization failed: {error}")))?;

        let requested_context = (prompt_tokens.len() as u32)
            .saturating_add(params.max_tokens)
            .saturating_add(8)
            .max(DEFAULT_CONTEXT_TOKENS);
        let context_tokens =
            NonZeroU32::new(requested_context).or_else(|| NonZeroU32::new(DEFAULT_CONTEXT_TOKENS));
        let mut context_params = LlamaContextParams::default().with_n_ctx(context_tokens);

        let threads = std::thread::available_parallelism()
            .ok()
            .and_then(|value| i32::try_from(value.get()).ok())
            .unwrap_or(4);
        context_params = context_params
            .with_n_threads(threads)
            .with_n_threads_batch(threads);

        let mut ctx = model
            .new_context(&self.backend, context_params)
            .map_err(|error| {
                InferError::Engine(format!("failed to create llama context: {error}"))
            })?;

        let required_context = prompt_tokens
            .len()
            .saturating_add(params.max_tokens as usize + 1);
        if required_context > ctx.n_ctx() as usize {
            return Err(InferError::Engine(format!(
                "context window too small: required {required_context} tokens, available {}",
                ctx.n_ctx()
            )));
        }

        let mut decoder = encoding_rs::UTF_8.new_decoder();

        let batch_capacity = std::cmp::max(prompt_tokens.len().saturating_add(1), 512);
        let mut batch = LlamaBatch::new(batch_capacity, 1);

        let last_prompt_index = prompt_tokens.len().saturating_sub(1);
        for (idx, token) in prompt_tokens.iter().copied().enumerate() {
            batch
                .add(
                    token,
                    i32::try_from(idx).map_err(|_| {
                        InferError::Engine("prompt too long for llama batch".to_string())
                    })?,
                    &[0],
                    idx == last_prompt_index,
                )
                .map_err(|error| {
                    InferError::Engine(format!("failed to add prompt token to batch: {error}"))
                })?;
        }

        ctx.decode(&mut batch).map_err(|error| {
            InferError::Engine(format!("llama decode failed for prompt prefill: {error}"))
        })?;

        let mut sampler = build_sampler(params);
        let mut n_cur = batch.n_tokens();
        let mut generated_tokens = 0_u32;

        while generated_tokens < params.max_tokens {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(EngineGenerationOutcome {
                    prompt_tokens: prompt_tokens.len() as u32,
                    generated_tokens,
                    finish_reason: GenerationFinishReason::Cancelled,
                });
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if model.is_eog_token(token) {
                return Ok(EngineGenerationOutcome {
                    prompt_tokens: prompt_tokens.len() as u32,
                    generated_tokens,
                    finish_reason: GenerationFinishReason::EndOfGenerationToken,
                });
            }

            let piece = model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|error| InferError::Engine(format!("failed to decode token: {error}")))?;

            if !piece.is_empty() {
                on_token(&piece);
            }

            generated_tokens = generated_tokens.saturating_add(1);

            batch.clear();
            batch.add(token, n_cur, &[0], true).map_err(|error| {
                InferError::Engine(format!("failed to add generated token to batch: {error}"))
            })?;
            n_cur += 1;

            ctx.decode(&mut batch).map_err(|error| {
                InferError::Engine(format!("llama decode failed during generation: {error}"))
            })?;
        }

        Ok(EngineGenerationOutcome {
            prompt_tokens: prompt_tokens.len() as u32,
            generated_tokens,
            finish_reason: GenerationFinishReason::MaxTokens,
        })
    }
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
        let clamped_top_p = params.top_p.min(1.0);
        chain.push(LlamaSampler::top_p(clamped_top_p, 1));
    }

    chain.push(LlamaSampler::temp(params.temperature));
    chain.push(LlamaSampler::dist(params.seed));

    LlamaSampler::chain_simple(chain)
}

pub type LlamaCppRuntime = EmbeddedInferenceRuntime<LlamaCppEngine>;
