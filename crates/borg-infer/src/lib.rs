use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use thiserror::Error;

pub type GenerationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardcodedModel {
    pub model_id: &'static str,
    pub gguf_path: &'static str,
}

const HARDCODED_MODELS: &[HardcodedModel] = &[
    HardcodedModel {
        model_id: "local/default",
        gguf_path: "/tmp/model.gguf",
    },
    HardcodedModel {
        model_id: "local/llama-3.1-8b-q4",
        gguf_path: "/tmp/llama-3.1-8b-q4.gguf",
    },
];

pub fn hardcoded_models() -> &'static [HardcodedModel] {
    HARDCODED_MODELS
}

pub fn hardcoded_model_path(model_id: &str) -> Option<PathBuf> {
    let model_id = model_id.trim();
    HARDCODED_MODELS
        .iter()
        .find(|entry| entry.model_id == model_id)
        .map(|entry| PathBuf::from(entry.gguf_path))
}

#[derive(Debug, Clone)]
pub struct GenerationParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub seed: u32,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: 128,
            temperature: 0.8,
            top_p: 0.95,
            top_k: 40,
            seed: 1234,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationFinishReason {
    EndOfGenerationToken,
    MaxTokens,
    Cancelled,
}

impl GenerationFinishReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EndOfGenerationToken => "end_of_generation_token",
            Self::MaxTokens => "max_tokens",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadReport {
    pub model_id: String,
    pub model_path: PathBuf,
    pub model_load_ms: u128,
    pub reloaded: bool,
}

#[derive(Debug, Clone)]
pub struct GenerationReport {
    pub generation_id: GenerationId,
    pub prompt_tokens: u32,
    pub generated_tokens: u32,
    pub generation_ms: u128,
    pub finish_reason: GenerationFinishReason,
}

impl GenerationReport {
    pub fn tokens_per_second(&self) -> f32 {
        if self.generation_ms == 0 {
            return self.generated_tokens as f32;
        }
        (self.generated_tokens as f32 * 1000.0) / self.generation_ms as f32
    }
}

#[derive(Debug, Error)]
pub enum InferError {
    #[error("embedded inference runtime is busy with generation {active_generation_id}")]
    Busy { active_generation_id: GenerationId },
    #[error("model `{model_id}` is not loaded")]
    ModelNotLoaded { model_id: String },
    #[error(
        "model mismatch: active model `{loaded_model_id}` but requested `{requested_model_id}`"
    )]
    ModelMismatch {
        loaded_model_id: String,
        requested_model_id: String,
    },
    #[error("no active generation")]
    NoActiveGeneration,
    #[error("generation `{generation_id}` is not active")]
    GenerationNotActive { generation_id: GenerationId },
    #[error("invalid model path `{path}`")]
    InvalidModelPath { path: String },
    #[error("internal runtime state is poisoned")]
    RuntimePoisoned,
    #[error("engine error: {0}")]
    Engine(String),
}

pub type Result<T> = std::result::Result<T, InferError>;

pub trait InferenceEngine: Send {
    fn load_model(&mut self, model_path: &Path) -> Result<()>;

    fn generate(
        &mut self,
        prompt: &str,
        params: &GenerationParams,
        cancelled: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<EngineGenerationOutcome>;
}

#[derive(Debug, Clone, Copy)]
pub struct EngineGenerationOutcome {
    pub prompt_tokens: u32,
    pub generated_tokens: u32,
    pub finish_reason: GenerationFinishReason,
}

pub trait InferenceRuntime {
    fn load(&self, model_id: &str, model_path: &Path) -> Result<LoadReport>;

    fn generate(
        &self,
        model_id: &str,
        prompt: &str,
        params: &GenerationParams,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<GenerationReport>;

    fn cancel(&self, generation_id: GenerationId) -> Result<()>;

    fn active_generation_id(&self) -> Option<GenerationId>;
}

pub struct EmbeddedInferenceRuntime<E>
where
    E: InferenceEngine,
{
    state: Arc<Mutex<RuntimeState<E>>>,
}

impl<E> Clone for EmbeddedInferenceRuntime<E>
where
    E: InferenceEngine,
{
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<E> EmbeddedInferenceRuntime<E>
where
    E: InferenceEngine,
{
    pub fn new(engine: E) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeState {
                engine: Some(engine),
                loaded_model: None,
                active_generation: None,
                next_generation_id: 1,
            })),
        }
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, RuntimeState<E>>> {
        self.state.lock().map_err(|_| InferError::RuntimePoisoned)
    }
}

impl<E> InferenceRuntime for EmbeddedInferenceRuntime<E>
where
    E: InferenceEngine,
{
    fn load(&self, model_id: &str, model_path: &Path) -> Result<LoadReport> {
        let model_id = model_id.trim().to_string();
        if model_id.is_empty() {
            return Err(InferError::ModelNotLoaded {
                model_id: String::new(),
            });
        }

        let mut state = self.lock_state()?;
        if let Some(active) = &state.active_generation {
            return Err(InferError::Busy {
                active_generation_id: active.generation_id,
            });
        }

        if state
            .loaded_model
            .as_ref()
            .is_some_and(|loaded| loaded.model_id == model_id && loaded.model_path == model_path)
        {
            return Ok(LoadReport {
                model_id,
                model_path: model_path.to_path_buf(),
                model_load_ms: 0,
                reloaded: false,
            });
        }

        let load_started = Instant::now();
        {
            let engine = state.engine.as_mut().ok_or_else(|| {
                InferError::Engine("engine missing from runtime state".to_string())
            })?;
            engine.load_model(model_path)?;
        }

        let load_ms = load_started.elapsed().as_millis();
        state.loaded_model = Some(LoadedModel {
            model_id: model_id.clone(),
            model_path: model_path.to_path_buf(),
        });

        Ok(LoadReport {
            model_id,
            model_path: model_path.to_path_buf(),
            model_load_ms: load_ms,
            reloaded: true,
        })
    }

    fn generate(
        &self,
        model_id: &str,
        prompt: &str,
        params: &GenerationParams,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<GenerationReport> {
        let (generation_id, cancel_flag, mut engine) = {
            let mut state = self.lock_state()?;

            let loaded_model =
                state
                    .loaded_model
                    .as_ref()
                    .ok_or_else(|| InferError::ModelNotLoaded {
                        model_id: model_id.to_string(),
                    })?;

            if loaded_model.model_id != model_id {
                return Err(InferError::ModelMismatch {
                    loaded_model_id: loaded_model.model_id.clone(),
                    requested_model_id: model_id.to_string(),
                });
            }

            if let Some(active) = &state.active_generation {
                return Err(InferError::Busy {
                    active_generation_id: active.generation_id,
                });
            }

            let generation_id = state.next_generation_id;
            state.next_generation_id = state.next_generation_id.saturating_add(1);

            let cancel_flag = Arc::new(AtomicBool::new(false));
            state.active_generation = Some(ActiveGeneration {
                generation_id,
                cancelled: cancel_flag.clone(),
            });

            let engine = state.engine.take().ok_or_else(|| {
                InferError::Engine("engine missing from runtime state".to_string())
            })?;

            (generation_id, cancel_flag, engine)
        };

        let generation_started = Instant::now();
        let generation_outcome = engine.generate(prompt, params, cancel_flag.as_ref(), on_token);
        let generation_ms = generation_started.elapsed().as_millis();

        {
            let mut state = self.lock_state()?;
            state.engine = Some(engine);
            if state
                .active_generation
                .as_ref()
                .is_some_and(|active| active.generation_id == generation_id)
            {
                state.active_generation = None;
            }
        }

        let outcome = generation_outcome?;
        Ok(GenerationReport {
            generation_id,
            prompt_tokens: outcome.prompt_tokens,
            generated_tokens: outcome.generated_tokens,
            generation_ms,
            finish_reason: outcome.finish_reason,
        })
    }

    fn cancel(&self, generation_id: GenerationId) -> Result<()> {
        let state = self.lock_state()?;
        let active = state
            .active_generation
            .as_ref()
            .ok_or(InferError::NoActiveGeneration)?;

        if active.generation_id != generation_id {
            return Err(InferError::GenerationNotActive { generation_id });
        }

        active.cancelled.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn active_generation_id(&self) -> Option<GenerationId> {
        self.lock_state().ok().and_then(|state| {
            state
                .active_generation
                .as_ref()
                .map(|active| active.generation_id)
        })
    }
}

struct RuntimeState<E>
where
    E: InferenceEngine,
{
    engine: Option<E>,
    loaded_model: Option<LoadedModel>,
    active_generation: Option<ActiveGeneration>,
    next_generation_id: GenerationId,
}

struct LoadedModel {
    model_id: String,
    model_path: PathBuf,
}

struct ActiveGeneration {
    generation_id: GenerationId,
    cancelled: Arc<AtomicBool>,
}

#[cfg(feature = "llama-cpp")]
mod llama_cpp {
    use super::{
        EmbeddedInferenceRuntime, EngineGenerationOutcome, GenerationFinishReason,
        GenerationParams, InferError, InferenceEngine, Result,
    };
    use std::num::NonZeroU32;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;

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
                .map_err(|error| {
                    InferError::Engine(format!("prompt tokenization failed: {error}"))
                })?;

            let requested_context = (prompt_tokens.len() as u32)
                .saturating_add(params.max_tokens)
                .saturating_add(8)
                .max(DEFAULT_CONTEXT_TOKENS);
            let context_tokens = NonZeroU32::new(requested_context)
                .or_else(|| NonZeroU32::new(DEFAULT_CONTEXT_TOKENS));
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
                    .map_err(|error| {
                        InferError::Engine(format!("failed to decode token: {error}"))
                    })?;

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
}

#[cfg(feature = "llama-cpp")]
pub use llama_cpp::{LlamaCppEngine, LlamaCppRuntime};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    #[derive(Clone)]
    struct TestProbe {
        loads: Arc<AtomicUsize>,
    }

    impl TestProbe {
        fn new() -> Self {
            Self {
                loads: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn load_count(&self) -> usize {
            self.loads.load(Ordering::Relaxed)
        }
    }

    struct ScriptedEngine {
        probe: TestProbe,
        sleep_per_token: Duration,
    }

    impl ScriptedEngine {
        fn new(probe: TestProbe, sleep_per_token: Duration) -> Self {
            Self {
                probe,
                sleep_per_token,
            }
        }
    }

    impl InferenceEngine for ScriptedEngine {
        fn load_model(&mut self, _model_path: &Path) -> Result<()> {
            self.probe.loads.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn generate(
            &mut self,
            prompt: &str,
            params: &GenerationParams,
            cancelled: &AtomicBool,
            on_token: &mut dyn FnMut(&str),
        ) -> Result<EngineGenerationOutcome> {
            let prompt_tokens = prompt.split_whitespace().count().max(1) as u32;
            let mut generated_tokens = 0_u32;

            while generated_tokens < params.max_tokens {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(EngineGenerationOutcome {
                        prompt_tokens,
                        generated_tokens,
                        finish_reason: GenerationFinishReason::Cancelled,
                    });
                }

                let token_chunk = format!("{}:{}", params.seed, generated_tokens);
                on_token(&token_chunk);
                generated_tokens += 1;

                if self.sleep_per_token > Duration::ZERO {
                    thread::sleep(self.sleep_per_token);
                }
            }

            Ok(EngineGenerationOutcome {
                prompt_tokens,
                generated_tokens,
                finish_reason: GenerationFinishReason::MaxTokens,
            })
        }
    }

    fn wait_for_active_generation<E>(runtime: &EmbeddedInferenceRuntime<E>) -> GenerationId
    where
        E: InferenceEngine,
    {
        for _ in 0..200 {
            if let Some(id) = runtime.active_generation_id() {
                return id;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for active generation id");
    }

    #[test]
    fn load_is_cached_for_same_model_id_and_path() {
        let probe = TestProbe::new();
        let runtime =
            EmbeddedInferenceRuntime::new(ScriptedEngine::new(probe.clone(), Duration::ZERO));

        let path = Path::new("/tmp/fake-model.gguf");

        let first = runtime
            .load("local/mock", path)
            .expect("first load should work");
        let second = runtime
            .load("local/mock", path)
            .expect("second load should work");

        assert!(first.reloaded);
        assert!(!second.reloaded);
        assert_eq!(probe.load_count(), 1);
    }

    #[test]
    fn generation_streams_chunks_and_is_deterministic_with_seed() {
        let probe = TestProbe::new();
        let runtime = EmbeddedInferenceRuntime::new(ScriptedEngine::new(probe, Duration::ZERO));
        runtime
            .load("local/mock", Path::new("/tmp/fake-model.gguf"))
            .expect("model load should work");

        let params = GenerationParams {
            max_tokens: 4,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 20,
            seed: 4242,
        };

        let mut first_output = String::new();
        let first_report = runtime
            .generate("local/mock", "hello world", &params, &mut |chunk| {
                first_output.push_str(chunk);
            })
            .expect("first generation should work");

        let mut second_output = String::new();
        let second_report = runtime
            .generate("local/mock", "hello world", &params, &mut |chunk| {
                second_output.push_str(chunk);
            })
            .expect("second generation should work");

        assert_eq!(first_output, second_output);
        assert!(first_output.contains("4242:0"));
        assert_eq!(first_report.generated_tokens, 4);
        assert_eq!(second_report.generated_tokens, 4);
    }

    #[test]
    fn cancel_stops_generation_in_flight() {
        let probe = TestProbe::new();
        let runtime =
            EmbeddedInferenceRuntime::new(ScriptedEngine::new(probe, Duration::from_millis(8)));
        runtime
            .load("local/mock", Path::new("/tmp/fake-model.gguf"))
            .expect("model load should work");

        let runtime_for_thread = runtime.clone();
        let handle = thread::spawn(move || {
            let params = GenerationParams {
                max_tokens: 64,
                ..GenerationParams::default()
            };
            let mut output = String::new();
            runtime_for_thread.generate("local/mock", "cancel me", &params, &mut |chunk| {
                output.push_str(chunk);
            })
        });

        let generation_id = wait_for_active_generation(&runtime);
        runtime
            .cancel(generation_id)
            .expect("cancel should be accepted");

        let report = handle
            .join()
            .expect("generation thread should not panic")
            .expect("generation should return report");
        assert_eq!(report.finish_reason, GenerationFinishReason::Cancelled);
        assert!(report.generated_tokens < 64);
    }

    #[test]
    fn second_generation_while_active_returns_busy() {
        let probe = TestProbe::new();
        let runtime =
            EmbeddedInferenceRuntime::new(ScriptedEngine::new(probe, Duration::from_millis(10)));
        runtime
            .load("local/mock", Path::new("/tmp/fake-model.gguf"))
            .expect("model load should work");

        let runtime_for_thread = runtime.clone();
        let handle = thread::spawn(move || {
            let params = GenerationParams {
                max_tokens: 32,
                ..GenerationParams::default()
            };
            runtime_for_thread.generate("local/mock", "long run", &params, &mut |_| {})
        });

        let _active_generation_id = wait_for_active_generation(&runtime);

        let err = runtime
            .generate(
                "local/mock",
                "another prompt",
                &GenerationParams::default(),
                &mut |_| {},
            )
            .expect_err("second generation should fail while busy");

        match err {
            InferError::Busy { .. } => {}
            other => panic!("expected busy error, got {other}"),
        }

        let first_result = handle
            .join()
            .expect("generation thread should not panic")
            .expect("first generation should complete");
        assert_eq!(
            first_result.finish_reason,
            GenerationFinishReason::MaxTokens
        );
    }
}
