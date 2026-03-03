use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use crate::engine::InferenceEngine;
use crate::types::{
    GenerationFinishReason, GenerationId, GenerationParams, GenerationReport, InferError,
    InitialState, LoadReport, Result, RunResult, RunSpec, RunSummary, RuntimeConfig,
};

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

#[derive(Clone)]
pub struct ConfiguredRuntime<E>
where
    E: InferenceEngine,
{
    runtime: EmbeddedInferenceRuntime<E>,
    config: RuntimeConfig,
}

impl<E> ConfiguredRuntime<E>
where
    E: InferenceEngine,
{
    pub fn new(engine: E, config: RuntimeConfig) -> Self {
        Self {
            runtime: EmbeddedInferenceRuntime::new(engine),
            config,
        }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub async fn run(&self, run_spec: RunSpec) -> Result<RunResult> {
        let executions = run_spec.executions.unwrap_or(self.config.executions);
        if executions == 0 {
            return Err(InferError::InvalidExecutions);
        }

        let params = run_spec
            .params
            .unwrap_or_else(|| self.config.default_params.clone());
        let load_report = self
            .runtime
            .load(&self.config.model_id, &self.config.gguf_path)?;

        let prompt = compose_prompt(&self.config.initial_state, &run_spec.input);
        let mut outputs = Vec::with_capacity(executions as usize);
        let mut generation_ids = Vec::with_capacity(executions as usize);
        let mut finish_reasons = Vec::with_capacity(executions as usize);
        let mut prompt_tokens_total = 0_u64;
        let mut generated_tokens_total = 0_u64;
        let mut generation_ms_total = 0_u128;

        for _ in 0..executions {
            let mut output = String::new();
            let report =
                self.runtime
                    .generate(&self.config.model_id, &prompt, &params, &mut |chunk| {
                        output.push_str(chunk);
                    })?;

            generation_ids.push(report.generation_id);
            finish_reasons.push(report.finish_reason);
            prompt_tokens_total =
                prompt_tokens_total.saturating_add(u64::from(report.prompt_tokens));
            generated_tokens_total =
                generated_tokens_total.saturating_add(u64::from(report.generated_tokens));
            generation_ms_total = generation_ms_total.saturating_add(report.generation_ms);
            outputs.push(output);

            if matches!(report.finish_reason, GenerationFinishReason::Cancelled) {
                break;
            }
        }

        let output = outputs.join("");
        let executions_completed = outputs.len() as u32;
        let tokens_per_second = if generation_ms_total == 0 {
            generated_tokens_total as f32
        } else {
            (generated_tokens_total as f32 * 1000.0) / generation_ms_total as f32
        };

        Ok(RunResult {
            output,
            outputs,
            summary: RunSummary {
                model_id: self.config.model_id.clone(),
                gguf_path: self.config.gguf_path.clone(),
                executions_requested: executions,
                executions_completed,
                model_load_ms: load_report.model_load_ms,
                model_reloaded: load_report.reloaded,
                generation_ids,
                prompt_tokens: prompt_tokens_total,
                generated_tokens: generated_tokens_total,
                generation_ms: generation_ms_total,
                tokens_per_second,
                finish_reasons,
            },
        })
    }
}

fn compose_prompt(initial_state: &InitialState, input: &str) -> String {
    let prefix = initial_state.prompt_prefix.trim();
    if prefix.is_empty() {
        return input.to_string();
    }
    format!("{prefix}\n{input}")
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
