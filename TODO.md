# TODO

This file tracks the current evals/agent runtime roadmap and the follow-up items we have explicitly agreed to.

## Current Priority

1. Finish eval timeouts
   - [x] Runtime `RunConfig` uses `Duration`.
   - [x] `evals.toml` supports `evals.timeout = "30s"` style durations.
   - [x] Timeout failures are structured in trial artifacts.

2. Finish removing remaining Ollama-local coupling
   - Normal eval execution should not depend on any Ollama-specific assumptions.
   - Hosted targets and local targets should both be cleanly supported without special-case runtime behavior.

3. Improve empty-config and filter errors
   - [x] `cargo evals run` fails clearly when there are no configured targets.
   - [x] Distinguish “no suites discovered”, “no targets configured”, and “no matches for the selected filters”.

4. Split `crates/evals/src/suite.rs`
   - `suite.rs` is still doing too much.
   - Likely split:
     - `suite/mod.rs`
     - `suite/planning.rs`
     - `suite/executor.rs`
     - `suite/target.rs`
     - `suite/trial.rs`
     - `suite/llm.rs`

5. Expand `AgentEvent` into the full runtime event stream
   - The current event model is still too output-oriented.
   - We want a complete stream of agent/runtime events so transcript recording can rely on the agent event stream directly.
   - [x] Pre-request system/context materialization is now emitted as an agent event.

6. Capture system/context materialization in transcripts
   - Transcripts still do not show the real system/context window seen by the model.
   - This depends on the fuller `AgentEvent` work.
   - [x] Persist `context_window` transcript rows when system context is materialized.

## Validation and Packaging

7. Keep validating external-workspace support
   - `borg-evals`, `borg-macros`, and `cargo-evals` must work from another project, not just this workspace.
   - Continue adding smoke coverage for:
     - external path dependencies
     - setup via `build.rs`
     - `cargo evals list`
     - `cargo evals run`

## Reporting and Cost Tracking

7. Add usage/cost tracking on `LlmRunner`
   - We want visibility into:
     - token counts
     - provider/model usage
     - spend/cost where available

## Future Work

8. Explore `agents::llm` support for Cloudflare Workers AI


---

Documentation checklist:
- [] Macros
    - [x] assistant
    - [x] setup
    - [x] trajectory
    - [x] user
- [] Structs
    - [x] AgentBuilder
    - [x] AgentTrial
    - [] AnthropicProviderConfig
    - [x] ArtifactIndex
    - [x] CallbackToolRunner
    - [] CompletionEventStream
    - [x] CompletionRequest
    - [x] CompletionRequestBuilder
    - [x] Builder for CompletionRequest.
    - [x] CompletionResponse
    - [x] ContextManager
    - [x] ContextManagerBuilder
    - [x] ContextWindow
    - [x] Eval
    - [x] EvalAggregate
    - [x] EvalContext
    - [x] EvalRunReport
    - [x] ExecutionProfile
    - [x] ExecutionTarget
    - [x] Grade
    - [x] GradeResult
    - [x] Grader
    - [x] GraderAggregate
    - [x] GraderFailure
    - [x] GradingConfig
    - [x] InMemoryStorageAdapter
    - [x] JsonEventSink
    - [x] JudgeAgent
    - [x] JudgeInput
    - [x] JudgeVerdict
    - [x] LlmRunner
    - [x] LlmRunnerBuilder
    - [] LmStudioProviderConfig
    - [x] NoToolRunner
    - [x] NoopEventSink
    - [] NoopStorageAdapter
    - [x] OllamaProviderConfig
    - [x] OpenAIProviderConfig
    - [x] OpenRouterProviderConfig
    - [x] PlannedSuiteRun
    - [] Probability
    - [x] ProgressEventSink
    - [x] ProviderConfigs
    - [x] RawCompletionEventStream
    - [x] RawCompletionRequest
    - [x] RawCompletionResponse
    - [] RecordedToolCall
    - [x] RunConfig
    - [x] RunManifest
    - [x] SessionAgent
    - [x] StaticContextProvider
    - [x] Step
    - [x] Suite
    - [] SuiteDescriptor
    - [x] SuiteRunReport
    - [x] SuiteSummary
    - [x] TargetFilter
    - [x] ToolCallEnvelope
    - [x] ToolResultEnvelope
    - [x] Trajectory
    - [x] TrajectoryBuilder
    - [x] TrialRecord
    - [] Usage
- [] Enums
    - [x] AgentError
    - [x] AgentEvent
    - [x] AgentInput
    - [x] CompletionEvent
    - [x] CompletionRequestBuilderError
    - [x] Error type for CompletionRequestBuilder
    - [x] ContextChunk
    - [x] ContextRole
    - [x] ContextStrategy
    - [x] EvalError
    - [x] FinishReason
    - [x] InputContent
    - [x] InputItem
    - [] ModelSelector
    - [] OutputContent
    - [] OutputItem
    - [] ProviderType
    - [] RawCompletionEvent
    - [] RawInputContent
    - [] RawInputItem
    - [] RawOutputContent
    - [] RawOutputItem
    - [x] RecordedError
    - [x] RecordedEvent
    - [x] RecordedGradingScope
    - [] RecordedMessageRole
    - [x] ResponseMode
    - [] Role
    - [x] RunEvent
    - [] StorageEvent
    - [] StorageInput
    - [] StorageRecord
    - [x] SuiteKind
    - [] Temperature
    - [] TokenLimit
    - [] ToolChoice
    - [] ToolExecutionResult
    - [] TopK
    - [] TopP
- [] Constants
    - [x] SCHEMA_VERSION
- [] Traits
    - [x] Agent
    - [x] ContextProvider
    - [x] EventSink
    - [] RunnableSuite
    - [x] StorageAdapter
    - [x] ToolRunner
- [] Functions
    - [] build
    - [x] emit
    - [x] global_sink
    - [x] grade
    - [x] judge
    - [x] predicate
    - [x] set_global_sink
- [] Type Aliases
    - [x] AgentResult
    - [x] AgentRunInput
    - [x] AgentRunOutput
    - [] EvalResult
    - [] SharedEventSink
- [] Attribute Macros
    - [x] eval
    - [x] grade
    - [x] suite
- [] Derive Macros
    - [x] Agent
    - [x] Tool
