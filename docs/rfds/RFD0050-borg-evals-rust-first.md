# RFD0050 - Rust-First Evals for `borg-agent` and `borg-llm`

- Feature Name: `borg-evals`
- Start Date: 2026-03-12
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary

Add a new `borg-evals` family of crates that lets Borg contributors define evaluation suites as ordinary Rust code, execute them against systems built on `borg-agent` and `borg-llm`, and persist stable, git-friendly result artifacts under `.evals/`.

The first version should be intentionally narrow:

- focused on systems built with `borg-agent` and `borg-llm`
- authored explicitly in Rust, without YAML-first design
- exposed as a library API only
- producing versioned JSON and Markdown artifacts from day one
- supporting repeated stochastic trials, deterministic graders, and suite-level baseline comparison

`cargo-evals`, proc-macro discovery, and even a dedicated CLI should come later. The first milestone is to make evals useful as code and durable as data.

## Motivation

We now have two foundational crates that make evaluation possible inside this repository:

- `borg-llm`, which provides typed provider-agnostic model execution
- `borg-agent`, which provides a typed session-oriented agent loop with tools, context, and streaming

The missing piece is an evaluation layer that feels native to Rust engineers and fits naturally into the existing codebase.

Today, if a team builds an agent or workflow on top of `borg-agent`, they still need to assemble their own ad hoc eval workflow:

- define cases somewhere outside the core code
- write one-off scripts to execute scenarios
- inspect raw transcripts manually
- compare runs by hand
- invent their own artifact layout
- infer regressions from logs rather than structured summaries

That is manageable for a few experiments, but it does not scale into normal engineering practice.

What teams actually need is:

- evals that can directly instantiate their agent/application code
- repeated trials to account for stochasticity
- graders that can inspect full transcripts and tool traces
- stable artifacts that can be checked into git like snapshots
- a baseline mechanism for commit-over-commit comparison

This is especially important for agent systems, where a final score alone is not enough. Engineers need to inspect:

- what messages were sent
- what tools were called
- what results came back
- where the grader found evidence for a pass or failure

The gap is not just “we need a score.” The gap is “we need evals to feel like ordinary Rust engineering, with durable evidence.”

## Guide-level explanation

### What an eval looks like

In the first version, an eval case should be an ordinary Rust scenario function.

It is not constrained to one prompt or one harness call. A case may:

- construct fixtures
- create an agent
- send multiple messages
- collect the full event stream
- emit artifacts
- return a typed trial output

A representative shape:

```rust
use borg_evals::prelude::*;

fn refunds_suite() -> Suite {
    Suite::new("support-refunds")
        .kind(SuiteKind::Regression)
        .trials(5)
        .case(
            Case::new("angry-customer-late-refund")
                .tag("support")
                .tag("refunds")
                .run(|ctx| async move {
                    let agent = fixtures::support_agent().await?;
                    let (tx, mut rx) = agent.run().await?;

                    tx.send_many([
                        AgentInput::Message("I was charged twice and I want my money back now."),
                    ])
                    .await?;

                    let transcript = rx.recv_all().await?;

                    Ok(AgentTrial {
                        transcript,
                    })
                })
                .grade(reply_contains("refund"))
                .grade(tool_sequence("verify before refund"))
        )
}
```

The important design point is that evals are code. If a contributor needs custom setup, fixtures, helper functions, or application-specific assertions, they should stay in Rust.

### Where evals live

Eval source should live near the crate it evaluates, with a layout similar to tests:

- `crates/<x>/evals/...`

Generated artifacts should live under a repository-level `.evals/` directory:

```text
.evals/
  baselines/
  results/
  artifacts/
```

This keeps:

- code close to the crate it exercises
- generated data centralized and easy to review

### How runs are compared

Each suite can produce a summary and can also have a blessed baseline.

The intended workflow is:

1. run an eval suite locally
2. inspect transcripts and grader evidence
3. improve prompts, tools, harness logic, or product code
4. rerun the suite
5. compare the latest suite summary to the blessed baseline
6. if the new run is the desired baseline, bless it

The CLI may eventually look like:

```text
cargo evals run support-refunds
cargo evals compare support-refunds
cargo evals bless <run-id>
```

But the first implementation should not start with a `cargo` subcommand or even a dedicated CLI. It should start with an explicit library API. Discovery and `cargo-evals` come later.

### What gets persisted

From the first version, eval results should be durable data, not just console output.

Versioned objects should include:

- run manifest
- suite summary
- case aggregate
- trial record
- artifact index

And trial artifacts should be rich enough to debug failures, not just score them.

For agent-oriented evals, that means first-class data for:

- transcript
- tool trace
- final reply
- errors
- timing
- grader evidence

### Repeated trials are first-class

A case may run multiple trials because the system under test is stochastic.

The framework should make that explicit:

- suites may define a default trial count
- cases may override it
- results should report pass rate, mean score, variance-related signals later, and grader-level breakdowns

The first implementation does not need advanced statistics, but it must model “the same case can be run multiple times” from the beginning.

## Reference-level explanation

## Scope

Version 1 is intentionally scoped to evaluating systems built on:

- `borg-agent`
- `borg-llm`

This is not yet a generic evaluation framework for arbitrary Rust systems.

That narrower scope should influence the design:

- trial outputs should work well for agent transcripts and model outputs
- graders should be able to inspect agent events and tool traces
- artifact schemas should reflect the semantics we already have in `borg-agent` and `borg-llm`

## Crate layout

The full family may eventually include:

### `borg-evals-core` (v0)

Core data model and eval authoring API.

Responsibilities:

- `Suite`
- `Case`
- `Trial`
- grading result model
- artifact and summary schemas
- result versioning

### `borg-evals-runner` (later)

Execution engine and artifact writer.

Responsibilities:

- explicit suite registration and execution
- repeated trial scheduling
- artifact writing
- baseline loading
- suite-level comparison

### `borg-evals-cli`

Command-line interface.

Responsibilities:

- run suites
- compare suites
- bless a baseline
- inspect a run

The CLI should come after the explicit runner API is working and proven useful.

### `borg-evals-graders` (later)

Built-in deterministic graders and utilities.

Responsibilities:

- boolean/predicate graders
- transcript graders
- tool sequence graders
- structured field graders
- artifact normalization helpers

LLM-as-a-judge should be future work, but the `Grader` API should be async from day one so it can be added later without redesigning the abstraction.

## Primary abstractions

The first version should keep the abstraction stack small.

The primary concepts are:

- `Suite`
- `Case`
- `Trial`
- `Grader`

The earlier idea of a separate `Evaluator` trait is likely unnecessary in v1 and should be omitted unless implementation proves otherwise.

Likewise, a generic `Harness` trait may be useful later, but should not be the central public model initially. In practice, an eval case for `borg-agent` is usually a scenario function, not a single pure harness invocation.

## `Suite`

A suite groups related cases and shared execution defaults:

- name
- kind
- description
- default trial count
- tags
- reporting metadata

The two suite kinds in v1 should be:

- `Regression`
- `Capability`

## `Case`

A case is a named scenario definition.

In v1, a case should be modeled as an async Rust function or closure over an eval context:

```rust
pub struct Case<O> {
    id: String,
    tags: Vec<String>,
    trial_count: Option<usize>,
    run: Arc<dyn Fn(CaseContext) -> BoxFuture<'static, EvalResult<O>> + Send + Sync>,
    graders: Vec<Arc<dyn Grader<O>>>,
}
```

The exact type details can vary, but the important semantic is:

- the case runs an arbitrary scenario
- the scenario returns a typed trial output `O`
- graders receive that same typed output

This allows rich agent scenarios:

- create an agent
- stream multiple inputs
- collect outputs
- return a structured `AgentTrial`

## `Trial`

A trial is one execution of one case.

Each trial should have:

- stable run/case/trial identity
- timing metadata
- trial output summary
- grader results
- artifact references
- pass/fail state

Since the same case may run multiple times, case aggregates should be derived from trial records rather than replacing them.

### `AgentTrial` in v1

Because v1 is explicitly scoped to systems built on `borg-agent` and `borg-llm`, the framework should define a first-class normalized trial shape for agent-oriented evals.

A minimal conceptual model:

```rust
pub struct AgentTrial {
    pub inputs: Vec<AgentTrialInput>,
    pub events: Vec<AgentTrialEvent>,
    pub final_reply: Option<Value>,
    pub error: Option<String>,
    pub metrics: TrialMetrics,
}

pub enum AgentTrialInput {
    Message { content: String },
    Steer { content: String },
    Cancel,
}

pub enum AgentTrialEvent {
    ModelOutputItem { item: Value },
    ToolCallRequested { call_id: String, name: String, args: Value },
    ToolExecutionCompleted { call_id: String, result: Value },
    Completed { reply: Value },
    Cancelled,
}

pub struct TrialMetrics {
    pub started_at: String,
    pub finished_at: String,
    pub latency_ms: u64,
}
```

The exact struct names may evolve, but the persisted trial model should preserve:

- the sequence of user/control inputs
- the sequence of emitted agent events
- the final reply when one exists
- terminal failure/cancellation information
- timing metadata

This should be enough for deterministic transcript-aware graders in v1.

## `Grader`

Graders should operate over typed outputs and return structured evidence.

A minimal shape:

```rust
#[async_trait]
pub trait Grader<O>: Send + Sync {
    fn name(&self) -> &'static str;
    async fn grade(&self, output: &O, ctx: &GradeContext) -> GradeResult;
}
```

Each grader returns:

- a scalar score
- pass/fail or threshold status
- evidence

Multiple dimensions should be represented as multiple graders, not sub-scores hidden inside one grader.

In v1, built-ins should be deterministic and programmatic:

- exact checks
- predicates
- transcript assertions
- tool sequence assertions
- structured field checks

### `GradeResult`

Each grader should return a stable, persistable result object.

A minimal shape:

```rust
pub struct GradeResult {
    pub name: String,
    pub score: f32,
    pub passed: bool,
    pub evidence: Vec<GradeEvidence>,
}

pub enum GradeEvidence {
    Note(String),
    Json(Value),
    ArtifactRef { name: String, path: String },
}
```

This keeps the grading contract small:

- one grader
- one scalar score
- one pass/fail interpretation
- explicit evidence

If multiple dimensions are needed, they should be represented as multiple graders.

## Typed execution, normalized persistence

The execution path should remain typed in Rust:

- cases return typed outputs
- graders receive typed outputs
- fixtures and helpers stay typed

But persisted artifacts and result records must be normalized and versioned.

That means:

- typed values are converted into stable JSON records at persistence boundaries
- all main persisted objects carry schema versioning from day one

This follows the same principle already established in the rest of Borg:

- typed internal execution
- stable boundary data

Concretely, this means:

- the scenario function can return a typed Rust output
- graders can inspect that typed Rust output
- the runner is responsible for lowering the relevant execution state into a normalized persisted record

For agent evals, the persisted representation should resemble the already-existing semantic event/history model in `borg-agent`, rather than opaque text snapshots.

## Suggested artifact model

The repository-level artifact layout should be stable from the first version.

Illustrative layout:

```text
.evals/
  baselines/
    support-refunds.json
  results/
    support-refunds/
      latest.json
      history/
        2026-03-12T11-42-00Z__abc123.json
      summaries/
        2026-03-12T11-42-00Z__abc123.md
  artifacts/
    support-refunds/
      2026-03-12T11-42-00Z__abc123/
        angry-customer-late-refund/
          trial-001/
            transcript.json
            output.json
            graders.json
            metrics.json
```

The exact filenames are not final, but the persisted object model should include:

- `RunManifest`
- `SuiteSummary`
- `CaseAggregate`
- `TrialRecord`
- `ArtifactIndex`

All of these should be versioned from day one.

### Minimal schema sketches

The exact fields can evolve, but the v1 objects should look roughly like this:

```rust
pub struct RunManifest {
    pub schema_version: String,
    pub run_id: String,
    pub suite_id: String,
    pub git_sha: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub case_ids: Vec<String>,
}

pub struct SuiteSummary {
    pub schema_version: String,
    pub run_id: String,
    pub suite_id: String,
    pub kind: SuiteKind,
    pub total_cases: usize,
    pub total_trials: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
    pub mean_latency_ms: f32,
    pub grader_deltas: Vec<GraderDelta>,
}

pub struct CaseAggregate {
    pub schema_version: String,
    pub run_id: String,
    pub case_id: String,
    pub trial_count: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
    pub mean_latency_ms: f32,
    pub graders: Vec<CaseGraderAggregate>,
}

pub struct TrialRecord {
    pub schema_version: String,
    pub run_id: String,
    pub case_id: String,
    pub trial_id: String,
    pub passed: bool,
    pub score: f32,
    pub output: Value,
    pub grader_results: Vec<GradeResult>,
    pub artifact_paths: Vec<String>,
}

pub struct ArtifactIndex {
    pub schema_version: String,
    pub run_id: String,
    pub case_id: String,
    pub trial_id: String,
    pub artifacts: Vec<ArtifactRef>,
}
```

The important commitment is not the exact field list. The important commitment is:

- all of these objects exist from v1
- all are versioned from v1
- all are stable enough to diff in git and compare across runs

## Baselines and blessing

The framework should support the concept of a blessed baseline.

For v1:

- baseline comparison is suite-level
- a suite has one current blessed baseline summary
- blessing a run means promoting that suite run to the baseline

This is conceptually similar to approving updated snapshot output in snapshot-testing workflows.

The intended CLI shape later is:

```text
cargo evals bless <run-id>
```

But the underlying model is simply:

- locate a completed run
- promote its suite summary into `.evals/baselines/`

## Comparison model

The first version should compare at the suite level.

Suite comparisons should include at least:

- pass rate delta
- mean score delta
- aggregate latency delta
- grader-by-grader deltas

Per-case and per-trial comparison can be future work, but the stored data should be rich enough to support them later.

## Initial implementation plan

The implementation should proceed in this order:

### Phase 0: smallest usable library

- `borg-evals-core`
- explicit Rust suite registration
- deterministic graders
- JSON and Markdown artifacts
- suite-level comparison
- one or two real agent-oriented suites in this repository

This should be enough to evaluate one real crate built on `borg-agent`.

The goal of this phase is to prove:

- cases are pleasant to write as Rust
- typed outputs and transcript-aware graders are sufficient
- artifacts are useful to inspect
- repeated trials and baselineable summaries are workable

No CLI or proc-macro support is required in this phase.

### Phase 1: runner and CLI

- `borg-evals-runner`
- `borg-evals-cli`
- `run`
- `compare`
- `inspect`
- `bless`

### Phase 2: ergonomics

- proc-macro registration
- `cargo-evals`
- richer built-in graders

The important constraint is:

- do not start with multiple crates unless the smallest library slice proves insufficient
- do not start with macro discovery or `cargo-evals`
- start with the core execution and artifact model

## Drawbacks

- This introduces another family of crates and schemas to maintain.
- Persisting artifacts in-repo by default may create noisy diffs and repository growth.
- The initial version is intentionally scoped to `borg-agent` and `borg-llm`, which means it is not immediately a universal eval framework.
- Versioning all major result objects from day one increases early design pressure.

## Rationale and alternatives

### Why Rust-first instead of YAML-first?

Because the target use case is not simple declarative cases. The target use case is:

- typed fixtures
- multi-turn agent scenarios
- direct use of application helpers
- transcript-aware grading

YAML may become a useful adapter later, but it is the wrong primary authoring model for v1.

### Why not start with `cargo-evals` and proc-macros?

Because that would optimize discovery before validating the execution model.

The first real risk is not “can we discover suites?” The first real risk is:

- do cases feel natural to write?
- are trial outputs and graders expressive enough?
- are artifacts useful to inspect?

Only once the explicit library path is good should automation be layered on.

### Why not design a generic framework immediately?

Because the immediate need is evaluating systems built on `borg-agent` and `borg-llm`.

A narrower scope reduces abstraction pressure and makes it much more likely that the first version is actually useful.

### Why not just use ordinary tests?

Ordinary tests do not naturally provide:

- repeated trials
- structured grader evidence
- durable eval artifacts
- baseline comparison workflows

`borg-evals` should complement tests, not replace them.

## Prior art

Useful precedents include:

- snapshot-testing workflows such as `cargo-insta`, especially the baseline approval model
- existing Python and TypeScript eval systems that treat transcripts and grader evidence as first-class artifacts
- internal Borg work on typed execution boundaries in `borg-agent` and `borg-llm`

The important lesson from these systems is:

- authored logic can remain typed and ergonomic
- persisted outputs must still be stable and reviewable

## Unresolved questions

- What exact field set should the first persisted schema versions contain?
- The first implementation should be library-only. The remaining question is when that should split into multiple crates instead of staying in `borg-evals-core`.
- How much of the normalized agent-trial shape should be built into core versus adapted by helper crates?
- Should v1 suite summaries include variance metrics, or only means and pass rates?
- How much report formatting belongs in core versus a later HTML/reporting crate?

## Future possibilities

- proc-macro registration like `#[evals]`
- `cargo-evals` subcommand integration
- a dedicated runner crate and CLI once the library surface stabilizes
- LLM-as-a-judge graders behind feature flags
- per-case and per-trial baseline comparison
- richer local inspection tools and HTML reports
- optional declarative adapters later, if code-first authoring proves too verbose for repetitive suites
- storage/indexing crates if local artifact volumes become large

The important future constraint is that these additions should build on the same core principle:

- evals are authored as code
- eval outputs are durable data
