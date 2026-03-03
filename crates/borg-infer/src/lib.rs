mod context_compiler;
mod engine;
mod llama_cpp;
mod model_catalog;
mod runtime;
mod types;

pub use context_compiler::{
    CompileParams, CompiledContext, CompiledGeneration, ContextCompiler, ContextCompilerBuilder,
};
pub use engine::{EngineGenerationOutcome, InferenceEngine};
pub use llama_cpp::{LlamaCppEngine, LlamaCppRuntime};
pub use model_catalog::{HardcodedModel, hardcoded_model_path, hardcoded_models};
pub use runtime::{ConfiguredRuntime, EmbeddedInferenceRuntime, InferenceRuntime};
pub use types::{
    GenerationFinishReason, GenerationId, GenerationParams, GenerationReport, InferError,
    InitialState, LoadReport, Result, RunResult, RunSpec, RunSpecBuilder, RunSummary,
    RuntimeConfig,
};

#[cfg(test)]
mod llama_cpp_tests;
#[cfg(test)]
mod tests;
