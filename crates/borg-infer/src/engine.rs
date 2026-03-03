use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::types::{GenerationFinishReason, GenerationParams, Result};

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
