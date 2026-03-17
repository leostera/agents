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
- persisting each completed trial immediately so partial runs remain durable
- executing against explicit provider/model targets rather than assuming a single local model matrix

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

### Execution targets, not just model strings

Real eval runs need to compare behavior across multiple execution environments:

- local Ollama models
- hosted OpenRouter models
- hosted Anthropic models
- hosted OpenAI models

So the unit of execution should not just be a model string. It should be an explicit execution target carrying, at minimum:

- provider
- model
- a human-friendly label
- concurrency policy

This matters because local and hosted targets have different runtime constraints:

- local targets are constrained by local machine resources and should default to sequential execution
- hosted targets are constrained more by network latency and quotas and should default to bounded concurrency
- hosted targets should be allowed to overlap with local targets rather than waiting behind them

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

The original generic design sketched `Case<O>` and `Grader<O>`. The prototype, however, taught a more useful lesson:

- v0 and likely v1 should stay explicitly agent-first
- the core case/grader surface can remain specialized to `AgentTrial`
- generalization should come later only if a second real trial shape forces it

So the practical v0/v1 semantic is:

- the case runs an arbitrary async Rust scenario
- the scenario returns an `AgentTrial`
- graders receive that same `AgentTrial`

This is still typed and expressive enough for the current target problem:

- create an agent
- stream multiple inputs
- collect outputs
- inspect tool traces
- return a structured `AgentTrial`

This should be treated as an intentional scope decision, not as an accident of the first prototype.

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

One concrete lesson from the calendar prototype is that failed trials must still preserve partial evidence whenever possible.

If an agent:

- calls tools
- emits partial assistant output
- or otherwise progresses meaningfully

and only later fails because it never produced a valid terminal reply, the persisted `TrialRecord` should still include the partial transcript and tool trace. Recording only `trial = null` and an error string is not sufficient for debugging weak models or malformed tool loops.

One concrete lesson from the first prototype is that tool-using agent evals must preserve provider-faithful replay state.

For follow-up turns after tool execution, valid replay may require:

- the original raw tool-call payload returned by the provider
- the typed Rust tool value used for execution
- the associated tool result
- provider-specific replay metadata such as tool names or exact argument shapes

It is not sufficient to only preserve typed Rust tool values and later reserialize them. Some providers require the exact original raw JSON arguments to successfully replay tool history.

## `Grader`

Graders should operate over the trial type used by core and return structured evidence.

For v0/v1, that means:

- `Grader` operates on `AgentTrial`
- built-in helpers stay agent-oriented
- later generalization can happen behind a descriptor layer if we truly need non-agent evals

The important contract is still:

- async grading
- stable grader name
- persistable structured result

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

The prototype also showed that browser-friendly grading will eventually need richer evidence than a free-form JSON blob. Future schema versions should likely support typed evidence entries such as:

- note
- JSON payload
- artifact reference
- transcript span or event reference

### Future `judge(...)` graders

V1 should remain deterministic and programmatic, but the grading abstraction should leave room for judge-based grading later.

The likely future shape is:

- a `judge(...)` helper layered on top of the same `Grader` abstraction
- a typed `JudgeResult` produced by a `borg-agent` agent or direct `borg-llm` runner
- a fold from `JudgeResult` into the stable persisted `GradeResult`

This is especially useful for dimensions that are awkward to grade with brittle string checks, such as:

- pleasantness
- tone or sentiment
- explanation quality
- clarity

Judge-based grading should remain auditable. Future persisted results should therefore record judge metadata such as:

- provider/model
- prompt or rubric version
- raw judge output when safe to persist

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

Another requirement proven by the prototype is incremental persistence:

- each completed trial should be written to disk immediately
- manifests and artifact indexes should be created early and updated as the run progresses
- final summaries can still be written at the end

This avoids losing a large run when a later trial crashes, the process exits unexpectedly, or a provider becomes unavailable partway through the suite.

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
    pub targets: Vec<ExecutionTarget>,
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
    pub suite_id: String,
    pub target: ExecutionTarget,
    pub case_id: String,
    pub trial_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub passed: bool,
    pub score: f32,
    pub output: Value,
    pub grader_results: Vec<GradeResult>,
    pub artifact_paths: Vec<String>,
}

pub struct ArtifactIndex {
    pub schema_version: String,
    pub run_id: String,
    pub target_label: String,
    pub case_id: String,
    pub trial_id: String,
    pub artifacts: Vec<ArtifactRef>,
}
```

`ExecutionTarget` should be explicit rather than implied:

```rust
pub struct ExecutionTarget {
    pub label: String,
    pub provider: String,
    pub model: String,
    pub max_in_flight: usize,
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

The first prototype also established a concrete concurrency rule:

- local targets should default to sequential execution
- hosted targets should default to bounded concurrent execution
- hosted targets should be allowed to overlap with local targets
- concurrency should apply across both cases and trials, with deterministic output ordering restored during persistence and reporting

The prototype also established a locality rule for local executors such as Ollama:

- local targets should run in a stable target order
- one local target should run to completion before the next local target begins
- this reduces repeated model load/unload churn across many long trial matrices
- hosted targets should still be allowed to run concurrently while a local target is active

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
- provider-faithful tool replay requirements are understood for tool-using agents
- incremental per-trial persistence is sufficient for long-running suites

No CLI or proc-macro support is required in this phase.

### Phase 1: runner and CLI

- `borg-evals-runner`
- `borg-evals-cli`
- `run`
- `compare`
- `inspect`
- `bless`
- better local progress output

### Phase 1.5: ergonomics and discovery

- `borg-evals-macros`
- thin proc-macro sugar for cases and suites
- convention-based suite discovery
- installable `cargo-evals` cargo subcommand
- local web explorer over `.evals/`

### Phase 2: ergonomics

- richer built-in graders
- retry/skip semantics for transient provider failures and rate limits
- `judge(...)` grading support

The important constraint is:

- do not start with multiple crates unless the smallest library slice proves insufficient
- do not start with macro discovery or `cargo-evals`
- start with the core execution and artifact model

## Ergonomics after core

The prototype clarified that the path to good DX should be layered, not magical.

The right progression is:

1. explicit core API
2. thin proc-macro sugar
3. convention-based cargo subcommand
4. local browser over the existing artifact model

This avoids coupling authoring convenience to execution semantics.

### Thin proc-macro layer

The proc-macro layer should stay intentionally thin.

Its job is to reduce boilerplate, not introduce a second runtime model.

The first useful macro shape is:

- `#[eval_case(...)]` on an async Rust function
- macro expands into an ordinary `Case` builder function
- the generated builder still returns a normal `Case`
- graders remain ordinary Rust for as long as possible

Conceptually:

```rust
#[eval_case(
    id = "compress-day",
    tags("calendar", "free-time"),
    trials = 20,
)]
async fn compress_day(ctx: TrialContext) -> EvalResult<AgentTrial> {
    // scenario body
}
```

expands into something equivalent to:

```rust
pub async fn compress_day(ctx: TrialContext) -> EvalResult<AgentTrial> { ... }

pub fn compress_day_case() -> Case {
    Case::new("compress-day")
        .tag("calendar")
        .tag("free-time")
        .trials(20)
        .run(|ctx| compress_day(ctx))
}
```

This gives the author:

- first-class Rust scenario functions
- IDE/navigation/refactor support
- no hidden registry magic

The same design should apply to suite sugar:

- the macro may generate a suite builder helper
- but the resulting object should still just be a `Suite`

The macro layer should not be responsible for discovery or execution.

### `cargo-evals` should be a convention-driven cargo subcommand

The cargo subcommand should come after the core API is proven, but its design is now clear enough to document.

`cargo-evals` should be a standalone cargo subcommand crate that:

- uses `cargo metadata` to inspect the workspace
- finds packages that follow the eval convention
- generates a temporary harness crate under `target/`
- compiles and runs that harness
- streams progress while the underlying library API persists artifacts to `.evals/`

The recommended workspace convention is:

- eval source lives under `crates/<pkg>/evals/`
- that tree exposes a small registration function such as:
  - `pub fn register(registry: &mut SuiteRegistry)`
  - or `pub fn suites() -> Vec<Suite>`

This implies a small descriptor layer should exist before the cargo subcommand lands:

- `SuiteMetadata`
- `CaseMetadata`
- `RegisteredSuite`
- `SuiteRegistry`

That registry layer is the clean seam between:

- authored suite code
- proc-macro sugar
- cargo-based discovery
- browser/index metadata

The harness generation step is important.

It avoids requiring:

- global linker-based registration
- `inventory`
- `linkme`
- hidden proc-macro side effects

Instead, `cargo-evals` can compile an explicit harness that imports the discovered eval modules by path or dependency and then invokes their normal Rust registration functions.

This keeps discovery understandable and debuggable.

It also fits the repository constraint that `borg-cli` remains the only primary product binary in this repo today. The later cargo subcommand should be treated as its own standalone tool rather than smuggled into the runtime surface.

The intended long-term commands are still:

```text
cargo evals run <suite-or-filter>
cargo evals compare <run-id>
cargo evals bless <run-id>
cargo evals inspect <run-id>
cargo evals serve
```

But the important design point is that these commands are only orchestration and presentation over the same core artifact model.

### Web-based results browser

The browser should be local-first and file-first.

The existing `.evals/` artifact layout is already close to the right storage model for a browser. The missing piece is a local-serving layer and a focused UI.

The recommended architecture is:

- `borg-evals-web`: static frontend assets
- `cargo-evals serve`: local HTTP server that:
  - serves the static app
  - exposes `.evals/` as a read-only API
  - optionally watches live runs and pushes updates

This should not start with a database.

The current file-first artifact model is already good enough for:

- runs list
- suite overview
- target comparison
- case detail
- trial detail
- transcript and tool trace inspection
- grader evidence inspection
- baseline comparison

The browser should be optimized for questions engineers actually ask:

- which target regressed?
- which cases are flaky?
- why did this trial fail?
- what tool sequence actually happened?
- how do two models differ on the same case?

Incremental persistence makes live inspection possible.

Because each trial is already written to disk immediately, a browser can observe a long-running suite while it is still executing. That should be treated as a design goal, not an accident.

The browser should therefore assume:

- run manifests appear first
- trial records arrive incrementally
- final summaries may land later

To support this cleanly, later schema revisions should add:

- a top-level run index
- explicit `trial_id` values
- typed artifact references instead of only raw file lists
- partial/final run state markers
- lazy-load-friendly trial references so large trajectories do not need to be embedded in summary objects

This fits long-running local and hosted eval runs naturally.

### Built-in agent helpers should follow the same rule

A few ergonomics should move out of examples and into the eval family once the core stabilizes:

- agent-trial capture helpers
- transcript/tool-trace normalization helpers
- deterministic graders for common agent checks
- later `judge(...)` helpers built on typed `JudgeResult -> GradeResult`

But these should still compile down to the same core model:

- `Case`
- `Suite`
- `TrialRecord`
- `GradeResult`

## Separation of concerns after v0

The prototype also made one architectural split clearer than the original draft:

- `borg-evals-core` currently mixes:
  - authoring
  - execution scheduling
  - artifact persistence

That is acceptable for v0, but it is not the desired long-term boundary.

The recommended next split is:

- `borg-evals-core`
  - authoring API
  - artifact/result schemas
  - metadata/registry types
- `borg-evals-runner`
  - scheduling
  - concurrency policy
  - retries/skips
  - persistence
  - comparisons
- `cargo-evals`
  - workspace discovery
  - harness generation
  - progress UI
  - browser serving

This means `cargo-evals` should not call deep into ad hoc `Suite::run*` internals forever. It should eventually have a runner-facing API designed for orchestration.

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

- How much partial trial state should be required on failed executions versus opportunistically captured when available?
- When should the explicit library-only runner split into `borg-evals-runner` and `cargo-evals`?
- What is the minimal stable registration convention for `crates/<pkg>/evals/`?
- Should the first browser be read-only, or should it also support actions like baseline blessing?
- How much summary formatting should remain in core once the browser and cargo subcommand exist?

## Future possibilities

- thin proc-macro registration over the core API
- `cargo-evals` subcommand integration
- a dedicated runner crate and CLI once the library surface stabilizes
- `judge(...)` graders behind feature flags
- per-case and per-trial baseline comparison
- richer local inspection tools and HTML reports
- web-based explorer over `.evals/` trajectories and comparisons
- optional declarative adapters later, if code-first authoring proves too verbose for repetitive suites
- storage/indexing crates if local artifact volumes become large

The important future constraint is that these additions should build on the same core principle:

- evals are authored as code
- eval outputs are durable data
