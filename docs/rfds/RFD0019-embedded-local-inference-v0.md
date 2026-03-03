# RFD0019: Embedded Local Inference v0 (GGUF + llama.cpp)

- Author: Leo
- Status: Draft
- Last updated: March 3, 2026

## Summary

Borg should support local LLM inference embedded directly in-process (no external Ollama/LM Studio runtime required). v0 focuses on one thing: load a GGUF model and generate tokens with streaming output and cancellation.

This RFD intentionally limits scope to prove the integration path before adding KV reuse, scheduling, memory composition, or provider/router wiring.

## Motivation

Borg already supports local external providers (for example LM Studio and Ollama), but deeper runtime goals require direct control over model execution:

1. cache-aware scheduling
2. memory-native context composition
3. tighter observability and lifecycle control

A minimal embedded path de-risks future work by validating GGUF + llama.cpp integration inside Borg codebase.

## Goals

1. Embedded inference in Borg codebase using `llama.cpp` via Rust.
2. Load GGUF from local filesystem.
3. Stream generation chunks as they are produced.
4. Support cancellation during generation.
5. Deterministic behavior under fixed seed in tests.
6. Keep initial implementation self-contained and simple.

## Non-goals (v0)

1. KV cache reuse across turns/sessions.
2. Prefix caching/segment reuse.
3. Tool calling in model runtime.
4. Multi-model scheduler/pool.
5. HTTP inference server mode.
6. Vision/multimodal.
7. Persistent model registry in DB.

## Design

### Package boundary

Create a dedicated crate: `crates/borg-infer`.

It owns:

1. runtime contract (`InferenceRuntime`)
2. engine contract (`InferenceEngine`)
3. generation state machine (single active generation, busy handling)
4. cancellation semantics by generation id
5. llama.cpp implementation (`LlamaCppEngine`) as the default engine for this crate

This keeps embedded inference isolated from `borg-llm` provider orchestration.

### Runtime contract (v0)

`InferenceRuntime` in `borg-infer`:

1. `load(model_id, model_path)`
2. `generate(model_id, prompt, params, on_token)`
3. `cancel(generation_id)`
4. `active_generation_id()`

`GenerationParams` includes:

1. `max_tokens`
2. `temperature`
3. `top_p`
4. `top_k`
5. `seed`

### Configured runtime API (v0)

`ConfiguredRuntime` is the high-level execution API:

1. configure once with `RuntimeConfig` (`model_id`, `gguf_path`, `initial_state`, defaults)
2. execute with `runtime.run(run_spec).await`
3. receive `RunResult` with:
   - `output` / `outputs`
   - serde-enabled `RunSummary` (`executions`, token counts, timings, finish reasons, model load info)

`RunSpec` uses a builder API:

1. `RunSpec::builder(input)`
2. optional setters for `executions`, `max_tokens`, `temperature`, `top_p`, `top_k`, `seed`

### Engine implementation

`LlamaCppEngine` uses `llama-cpp-2` and GGUF files:

1. initialize backend
2. load model
3. tokenize prompt
4. decode prefill
5. sampling loop with chunk callbacks
6. stop on EOS, max tokens, or cancel flag

### Concurrency policy (v0)

1. exactly one loaded model in runtime instance
2. exactly one active generation
3. second generate call while active returns `Busy`

### Model catalog (v0)

No DB-backed model registry in v0. `borg-infer` ships a hardcoded catalog and allows explicit GGUF path override from CLI.

## CLI surface (v0)

1. `borg infer models`
   - lists hardcoded model ids and default GGUF paths from `borg-infer`
2. `borg infer run <path-to-gguf> <input_text> [--model-id ...] [--executions ...] [--initial-prefix ...] [--max-tokens ...] [--temperature ...] [--top-p ...] [--top-k ...] [--seed ...]`
   - runs local inference via `ConfiguredRuntime::run`
   - prints structured JSON to stdout (compatible with `jq`)
   - sampling/execution parameters are configured from CLI flags into `RunSpec`
3. `borg providers set default embedded`
   - sets runtime preferred provider id to `embedded`
   - provider-runtime wiring remains follow-up work

## Testing

`borg-infer` unit tests cover:

1. model load caching behavior
2. streaming chunk emission
3. deterministic output with fixed seed
4. cancellation during active generation
5. busy error when concurrent generation is attempted
6. `RunSpec` builder behavior and `RunSummary` serde roundtrip
7. `LlamaCppEngine` missing-file validation
8. `LlamaCppEngine` smoke generation with a real model when `BORG_INFER_TEST_GGUF` is set

## Observability (v0)

`borg infer run` emits a JSON payload containing `RunResult.summary` with key operational fields:

1. `model_load_ms`
2. `prompt_tokens`
3. `generated_tokens`
4. `generation_ms`
5. `tokens_per_second`
6. `finish_reasons`

## Risks

1. `llama-cpp-2` is now a direct dependency and introduces native build/toolchain requirements (notably clang).
2. runtime currently single-generation only.
3. hardcoded model catalog is operationally limited and temporary.

## Rollout

### Phase 0

1. land `borg-infer` crate and CLI smoke path
2. keep model config self-contained/hardcoded
3. do not wire into session/provider routing yet

### Follow-up

1. DB-backed model registry
2. provider resolver integration for `embedded`
3. runtime actor/service integration for session turns
4. KV cache and scheduler work

## References

1. `llama.cpp`: <https://github.com/ggerganov/llama.cpp>
2. `llama-cpp-2`: <https://github.com/utilityai/llama-cpp-rs>
3. GGUF docs: <https://huggingface.co/docs/hub/gguf>
