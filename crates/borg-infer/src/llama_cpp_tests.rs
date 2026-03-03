use std::path::{Path, PathBuf};

use crate::LlamaCppEngine;
use crate::engine::InferenceEngine;
use crate::runtime::{EmbeddedInferenceRuntime, InferenceRuntime};
use crate::types::{GenerationFinishReason, GenerationParams, InferError};

fn llama_cpp_test_model_path() -> Option<PathBuf> {
    let path = std::env::var_os("BORG_INFER_TEST_GGUF")?;
    let path = PathBuf::from(path);
    if path.is_file() { Some(path) } else { None }
}

#[test]
fn llama_cpp_engine_rejects_missing_model_file() {
    let mut engine = LlamaCppEngine::new().expect("llama backend init should work");
    let err = engine
        .load_model(Path::new("/tmp/borg-infer-missing-test-model.gguf"))
        .expect_err("missing GGUF should fail");

    match err {
        InferError::InvalidModelPath { .. } => {}
        other => panic!("expected InvalidModelPath, got {other}"),
    }
}

#[test]
fn llama_cpp_engine_smoke_generate_with_real_model() {
    let Some(model_path) = llama_cpp_test_model_path() else {
        eprintln!(
            "skipping llama_cpp_engine_smoke_generate_with_real_model: set BORG_INFER_TEST_GGUF to a GGUF file"
        );
        return;
    };

    let runtime = EmbeddedInferenceRuntime::new(
        LlamaCppEngine::new().expect("llama backend init should work"),
    );
    runtime
        .load("local/smoke", &model_path)
        .expect("model load should succeed");

    let params = GenerationParams {
        max_tokens: 16,
        temperature: 0.0,
        top_p: 1.0,
        top_k: 0,
        seed: 7,
    };
    let mut output = String::new();
    let report = runtime
        .generate(
            "local/smoke",
            "hello from borg infer llama.cpp test",
            &params,
            &mut |chunk| {
                output.push_str(chunk);
            },
        )
        .expect("generation should succeed");

    assert!(report.prompt_tokens > 0);
    assert!(report.generated_tokens <= params.max_tokens);
    assert!(matches!(
        report.finish_reason,
        GenerationFinishReason::EndOfGenerationToken | GenerationFinishReason::MaxTokens
    ));
    assert!(
        !output.is_empty() || report.finish_reason == GenerationFinishReason::EndOfGenerationToken,
        "expected token output unless generation terminated immediately with EOG"
    );
}
