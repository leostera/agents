# TODO Audit

This file records the TODOs encountered during the current cleanup pass.
The intent is to keep the reasoning visible even after the original inline TODO comment is removed.

## Legend

- `addressed`: the underlying issue has been fixed in code
- `in_progress`: code is actively being refactored but not yet validated
- `open`: the underlying issue is still real and needs implementation
- `rejected`: the original TODO direction was wrong and was intentionally not implemented

## Inventory

1. `crates/borg-evals/src/grade.rs`
   Status: `addressed`
   Context: the grading surface duplicated grader identity in `GradeResult.name`, relied on helper constructors like `pass_if`, and carried a custom `IntoGradingConfig` trait just to make the API chain.
   Implemented fix:
   - `Grader` owns the grade name
   - `GradeResult` is plain data returned from normal Rust code
   - `pass_if`/builder-style helpers are gone from the authored surface
   - trial/report grade storage now keys grades by grader name instead of duplicating names inside `GradeResult`

2. `crates/borg-evals/src/suite.rs`
   Status: `addressed`
   Context: target execution was hard-coded directly into `SuiteRunner::run`, with a design note about eventually splitting this into a local executor and a remote executor.
   Implemented fix:
   - introduce a real `SuiteExecutor` seam
   - implement `LocalExecutor`
   - have `SuiteRunner` delegate through that seam

3. `crates/borg-evals/examples/calculator_agent.rs`
   Status: `addressed`
   Context: tool definition wiring is still handwritten; the example was pointing at a future derive-based tool macro.
   Implemented fix:
   - added `#[derive(borg_macros::AgentTool)]` for tool enums
   - replaced the manual `TypedTool` impl in the calculator example
   - argument payload types remain ordinary serde/schemars Rust types

4. `crates/test-agents/evals/echo.rs`
   Status: `addressed`
   Context: reusable deterministic graders were still being hand-written inline as closures, and the file explicitly wanted a reusable grade abstraction.
   Implemented fix:
   - add a `#[grade]` proc macro that wraps a plain async Rust function into a reusable `Grader`
   - moved the echo suite onto reusable grade functions

5. `crates/test-agents/evals/echo.rs`
   Status: `addressed`
   Context: one-step trajectory authoring was too noisy for hand-written evals.
   Implemented fix:
   - kept the linear runtime model but simplified the authored one-step path to `Trajectory::new(Step::user(...).grade(...))`
   - moved the echo suite onto that simpler path

6. `crates/borg-agent/src/agent.rs`
   Status: `addressed`
   Context: `Agent` bounds only lived on impl blocks, and helper logic for abandoned tool calls / context chunk conversion was spread across free functions.
   Implemented fix:
   - moved the runtime bounds onto `Agent`
   - moved pending-tool traversal onto `TurnState`
   - moved chunk conversion onto `ToolCallEnvelope` / `ToolResultEnvelope`

7. `crates/borg-llm/src/tools.rs`
   Status: `addressed`
   Context: tool metadata types had awkward `Raw*` names and a Rust field called `r#type`, even though this is ordinary structured metadata in Rust code.
   Implemented fix:
   - introduced `ToolDefinition`, `ToolFunction`, and `ToolSet`
   - renamed the Rust field to `kind` while preserving wire-format `"type"`
   - kept compatibility aliases where useful

8. `crates/borg-llm/src/tools.rs`
   Status: `addressed`
   Context: `TypedToolSet` had a TODO questioning whether it should exist at all.
   Implemented fix:
   - kept the concept because it is still used to materialize typed tool metadata at request construction time
   - renamed/documented it as `ToolSet` and left the old name as a compatibility alias

9. `evals.toml`
   Status: `open`
   Context: hosted targets are still commented out because the current local harness path is too tied to container-managed Ollama setup.
   Intended fix:
   - decouple provider selection from Ollama container startup
   - allow local/server-backed Ollama without forcing container startup

10. `crates/borg-evals/src/grade.rs`
    Status: `rejected`
    Context: one intermediate TODO suggested making `GradeResult` fields private.
    Reason rejected:
    - deterministic graders should read like ordinary Rust code
    - hiding the fields would push the API back toward a builder DSL

## Verification Checklist

- [x] `rg -n "TODO\\(|TODO|FIXME|XXX" crates evals.toml` is empty
- [x] every `addressed` item above is backed by compiling code and tests
- [x] every remaining `open` item remains visible here instead of being deleted
