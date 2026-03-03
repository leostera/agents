use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type GenerationId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadReport {
    pub model_id: String,
    pub model_path: PathBuf,
    pub model_load_ms: u128,
    pub reloaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub model_id: String,
    pub gguf_path: PathBuf,
    #[serde(default)]
    pub initial_state: InitialState,
    #[serde(default)]
    pub default_params: GenerationParams,
    #[serde(default = "default_executions")]
    pub executions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InitialState {
    #[serde(default)]
    pub prompt_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSpec {
    pub input: String,
    #[serde(default)]
    pub params: Option<GenerationParams>,
    #[serde(default)]
    pub executions: Option<u32>,
}

impl RunSpec {
    pub fn builder(input: impl Into<String>) -> RunSpecBuilder {
        RunSpecBuilder::new(input)
    }
}

#[derive(Debug, Clone)]
pub struct RunSpecBuilder {
    input: String,
    params: Option<GenerationParams>,
    executions: Option<u32>,
}

impl RunSpecBuilder {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            params: None,
            executions: None,
        }
    }

    pub fn params(mut self, params: GenerationParams) -> Self {
        self.params = Some(params);
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.params_mut().max_tokens = max_tokens;
        self
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.params_mut().temperature = temperature;
        self
    }

    pub fn top_p(mut self, top_p: f32) -> Self {
        self.params_mut().top_p = top_p;
        self
    }

    pub fn top_k(mut self, top_k: i32) -> Self {
        self.params_mut().top_k = top_k;
        self
    }

    pub fn seed(mut self, seed: u32) -> Self {
        self.params_mut().seed = seed;
        self
    }

    pub fn executions(mut self, executions: u32) -> Self {
        self.executions = Some(executions);
        self
    }

    pub fn build(self) -> RunSpec {
        RunSpec {
            input: self.input,
            params: self.params,
            executions: self.executions,
        }
    }

    fn params_mut(&mut self) -> &mut GenerationParams {
        self.params.get_or_insert_with(GenerationParams::default)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub model_id: String,
    pub gguf_path: PathBuf,
    pub executions_requested: u32,
    pub executions_completed: u32,
    pub model_load_ms: u128,
    pub model_reloaded: bool,
    pub generation_ids: Vec<GenerationId>,
    pub prompt_tokens: u64,
    pub generated_tokens: u64,
    pub generation_ms: u128,
    pub tokens_per_second: f32,
    pub finish_reasons: Vec<GenerationFinishReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub output: String,
    pub outputs: Vec<String>,
    pub summary: RunSummary,
}

fn default_executions() -> u32 {
    1
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
    #[error("executions must be greater than zero")]
    InvalidExecutions,
    #[error("internal runtime state is poisoned")]
    RuntimePoisoned,
    #[error("engine error: {0}")]
    Engine(String),
}

pub type Result<T> = std::result::Result<T, InferError>;
