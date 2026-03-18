# borg-evals

Typed eval runtime, report artifacts, suite registry generation, and the shared runner used by `cargo-evals`.

## Structure

```
src/
├── eval.rs        # Eval, EvalAgent, EvalContext
├── eval.rs        # Eval and EvalContext
├── suite.rs       # Suite orchestration and trial execution
├── trajectory.rs  # Linear trajectory builder and runner
├── grade.rs       # Grader, GradingConfig, GradeResult, predicate helper
├── judge.rs       # built-in JudgeAgent and judge(...) grader helper
├── events.rs      # RunEvent schema and global event sink
├── runner/        # Workspace discovery, config loading, harness generation, run/list flow
├── registry.rs    # build.rs support plus generated registry helpers
├── report.rs      # persisted reports and aggregates
└── trial.rs       # AgentTrial recording and serialized trial artifacts
```

## Key Contracts

### Suite ownership
- `Suite<State, Agent>` owns the shared state and the agent factory.
- `Eval<State, Agent>` owns one scenario plus its grader/runner.
- Keep the agent type anchored at the suite level unless there is a strong reason to widen the surface.

### Trajectories
- `Trajectory<Agent, State>` is the declarative runner path.
- Step expectations grade partial trials immediately and those grades must be preserved even when the trial fails later.
- Record trajectory step inputs and grading lifecycle into the trial transcript so failed artifacts explain both what was sent and how it was graded.
- `trajectory.runner()` should remain a thin closure adapter over the typed runtime.
- For hand-written one-step evals, prefer `Trajectory::new(Step::user(...).grade(...))` over the full builder.

### Grading
- Deterministic graders should read like ordinary Rust code returning `GradeResult { score, summary, evidence }`.
- Prefer `predicate(name, |trial, ctx| ...)` for deterministic grading and `judge(name, rubric)` for LLM-based grading.
- `Grader` owns the grade name; `GradeResult` should not duplicate it.
- Prefer reusable `#[borg_macros::grade]` functions when the same deterministic grader is used in more than one eval.

### Reports and artifacts
- Runtime stays typed through `AgentTrial<Output>`.
- Reports erase to JSON only at the artifact boundary.
- `AgentTrial.transcript` should include trajectory step boundaries plus grader start/completion/failure events, not just raw agent output.
- Trial logs and artifact filenames should always carry `trial_id` so terminal output maps back to `.evals` files.

### Runner split
- `cargo-evals` is intentionally thin.
- Workspace discovery, `evals.toml` loading, harness preparation, and output formatting belong under `borg_evals::runner`.
- The generated harness should stay minimal and delegate real behavior back into `borg-evals`.

### Terminal output
- `RunEvent` is the shared event transport.
- `--json` should emit line-delimited JSON events.
- Human-facing output should be driven from the same event stream, not ad hoc tracing logs.

### Proc macros
- `#[suite]` and `#[eval]` live in `borg-macros`, but they generate `borg-evals` runtime objects.
- `#[grade]` should stay thin: take a normal async Rust function and wrap it as a reusable `Grader`.
- `#[derive(Tool)]` targets tool enums; keep the generated `TypedTool` impl predictable and inspectable.

## Crate integration

Package-side setup is:

```rust
// build.rs
fn main() -> anyhow::Result<()> {
    borg_evals::build()?;
    Ok(())
}
```

```rust
// src/lib.rs
borg_evals::setup!();
```

Suite sources live under `evals/**/*.rs` and are discovered during `build.rs`.

## Commands

```bash
cargo build -p borg-evals
cargo test -p borg-evals
cargo evals list
cargo evals run
```
