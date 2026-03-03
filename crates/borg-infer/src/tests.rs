use crate::engine::{EngineGenerationOutcome, InferenceEngine};
use crate::runtime::{ConfiguredRuntime, EmbeddedInferenceRuntime, InferenceRuntime};
use crate::types::{
    GenerationFinishReason, GenerationId, GenerationParams, InferError, InitialState, Result,
    RunSpec, RunSummary, RuntimeConfig,
};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    let runtime = EmbeddedInferenceRuntime::new(ScriptedEngine::new(probe.clone(), Duration::ZERO));

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

#[test]
fn runspec_builder_sets_optional_fields() {
    let run_spec = RunSpec::builder("hello from builder")
        .executions(4)
        .max_tokens(9)
        .temperature(0.3)
        .top_p(0.8)
        .top_k(12)
        .seed(7)
        .build();

    assert_eq!(run_spec.input, "hello from builder");
    assert_eq!(run_spec.executions, Some(4));
    let built_params = run_spec.params.expect("builder should set params");
    assert_eq!(built_params.max_tokens, 9);
    assert_eq!(built_params.temperature, 0.3);
    assert_eq!(built_params.top_p, 0.8);
    assert_eq!(built_params.top_k, 12);
    assert_eq!(built_params.seed, 7);
}

#[tokio::test]
async fn configured_runtime_run_returns_summary_and_serializes() {
    let probe = TestProbe::new();
    let config = RuntimeConfig {
        model_id: "local/mock".to_string(),
        gguf_path: PathBuf::from("/tmp/fake-model.gguf"),
        initial_state: InitialState {
            prompt_prefix: "system prefix".to_string(),
        },
        default_params: GenerationParams {
            max_tokens: 3,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 20,
            seed: 99,
        },
        executions: 2,
    };
    let runtime =
        ConfiguredRuntime::new(ScriptedEngine::new(probe.clone(), Duration::ZERO), config);

    let result = runtime
        .run(RunSpec::builder("hello").executions(3).build())
        .await
        .expect("configured runtime run should work");

    assert_eq!(probe.load_count(), 1);
    assert_eq!(result.outputs.len(), 3);
    assert_eq!(result.summary.executions_requested, 3);
    assert_eq!(result.summary.executions_completed, 3);
    assert_eq!(result.summary.finish_reasons.len(), 3);
    assert_eq!(result.summary.generation_ids.len(), 3);
    assert!(result.summary.generated_tokens > 0);

    let encoded = serde_json::to_string(&result.summary).expect("summary should serialize");
    let decoded: RunSummary = serde_json::from_str(&encoded).expect("summary should deserialize");
    assert_eq!(decoded.executions_requested, 3);
    assert_eq!(decoded.executions_completed, 3);
}

#[tokio::test]
async fn configured_runtime_rejects_zero_executions() {
    let probe = TestProbe::new();
    let config = RuntimeConfig {
        model_id: "local/mock".to_string(),
        gguf_path: PathBuf::from("/tmp/fake-model.gguf"),
        initial_state: InitialState::default(),
        default_params: GenerationParams::default(),
        executions: 1,
    };
    let runtime = ConfiguredRuntime::new(ScriptedEngine::new(probe, Duration::ZERO), config);

    let err = runtime
        .run(RunSpec::builder("hello").executions(0).build())
        .await
        .expect_err("zero executions should fail");
    match err {
        InferError::InvalidExecutions => {}
        other => panic!("expected InvalidExecutions, got {other}"),
    }
}
