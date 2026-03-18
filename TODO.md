# TODO

This file tracks the current evals/agent runtime roadmap and the follow-up items we have explicitly agreed to.

## Current Priority

1. Finish eval timeouts
   - Timeout plumbing exists, but the end-to-end behavior and UX still need tightening.
   - Keep the API in `Duration`, and make the failure mode obvious in both terminal output and trial artifacts.

2. Finish removing remaining Ollama-local coupling
   - Normal eval execution should not depend on any Ollama-specific assumptions.
   - Hosted targets and local targets should both be cleanly supported without special-case runtime behavior.

3. Improve empty-config and filter errors
   - `cargo evals run` should fail clearly when there are no configured targets.
   - Distinguish “no suites discovered”, “no targets configured”, and “no matches for the selected filters”.

4. Split `crates/borg-evals/src/suite.rs`
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

6. Capture system/context materialization in transcripts
   - Transcripts still do not show the real system/context window seen by the model.
   - This depends on the fuller `AgentEvent` work.

## Validation and Packaging

7. Keep validating external-workspace support
   - `borg-evals`, `borg-macros`, and `cargo-evals` must work from another project, not just this workspace.
   - Continue adding smoke coverage for:
     - external path dependencies
     - setup via `build.rs`
     - `cargo evals list`
     - `cargo evals run`

8. Feature-flag `borg_llm::testing`
   - `borg-llm` should not pull `testcontainers` into normal binary dependency resolution.
   - The testing helpers should be behind an explicit feature or otherwise isolated to test-only use.

## Reporting and Cost Tracking

9. Add usage/cost tracking on `LlmRunner`
   - We want visibility into:
     - token counts
     - provider/model usage
     - spend/cost where available

## Future Work

10. Explore `borg-llm` support for Cloudflare Workers AI


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
    - [] CompletionRequestBuilder
    - [] Builder for CompletionRequest.
    - [] CompletionResponse
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
    - [] NoopEventSink
    - [] NoopStorageAdapter
    - [x] OllamaProviderConfig
    - [x] OpenAIProviderConfig
    - [x] OpenRouterProviderConfig
    - [x] PlannedSuiteRun
    - [] Probability
    - [] ProgressEventSink
    - [x] ProviderConfigs
    - [] RawCompletionEventStream
    - [] RawCompletionRequest
    - [] RawCompletionResponse
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
    - [] CompletionEvent
    - [] CompletionRequestBuilderError
    - [] Error type for CompletionRequestBuilder
    - [x] ContextChunk
    - [x] ContextRole
    - [x] ContextStrategy
    - [] EvalError
    - [] FinishReason
    - [] InputContent
    - [] InputItem
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
    - [] ResponseMode
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
    - [] SCHEMA_VERSION
- [] Traits
    - [x] Agent
    - [x] ContextProvider
    - [] EventSink
    - [] RunnableSuite
    - [x] StorageAdapter
    - [x] ToolRunner
- [] Functions
    - [] build
    - [] emit
    - [] global_sink
    - [x] grade
    - [x] judge
    - [x] predicate
    - [] set_global_sink
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
