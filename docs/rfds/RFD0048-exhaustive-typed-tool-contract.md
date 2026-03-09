# RFD0048 - Exhaustive Typed Tool Contract

- Feature Name: `exhaustive_typed_tool_contract`
- Start Date: `2026-03-09`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

This RFD enforces strong typing for all tool calls and results across the Borg runtime. It eliminates the use of `serde_json::Value` in core dispatch logic by introducing exhaustive enums that represent the entire tool ecosystem.

1. All tool arguments and results must be defined as typed Rust structs.
2. A central `BorgToolCall` and `BorgToolResult` enum in `borg-core` will aggregate all possible tools.
3. The `borg-agent` loop will perform **immediate parsing** from LLM JSON into these exhaustive enums at the execution edge.
4. Validation errors during parsing will be fed back to the LLM immediately, preventing malformed data from entering the runtime hot-paths.

## Motivation
[motivation]: #motivation

Currently, `borg-agent` uses generic `TToolCall` and `TToolResult` parameters, which in the production implementation have regressed to wrappers around `serde_json::Value`. This creates several issues:

1. **Meaningless Payloads**: Data flowing through `borg-exec`, `BorgActorManager`, and the `messages` table is untyped JSON, violating the "JSON only at the edges" mandate of RFD0017.
2. **Fragile Dispatch**: Tool dispatch depends on string matching and late-stage deserialization, leading to hard-to-debug runtime failures.
3. **Weak Feedback**: When an LLM generates a malformed tool call, the system often fails with a generic error instead of providing precise, type-aware feedback that could help the LLM correct its mistake.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### The "Parse at the Edge" Pattern

The runtime will treat the LLM response as an untrusted external boundary. As soon as a tool call is received, it is converted into a strongly-typed variant of the `BorgToolCall` enum.

```rust
// crates/borg-core/src/tool_contract.rs

#[derive(Serialize, Deserialize)]
#[serde(tag = "tool", content = "args", rename_all = "kebab-case")]
pub enum BorgToolCall {
    ActorsListActors(ListActorsArgs),
    ActorsSendMessage(SendMessageArgs),
    CodeModeExecuteCode(ExecuteCodeArgs),
    // ... all other tools
}
```

### Strict LLM Feedback Loop

In `borg-agent::Agent::run`, the processing logic changes from:
* "Receive JSON -> Wrap in Value -> Send to Exec"
to:
* "Receive JSON -> **Try Parse to BorgToolCall** -> On Error: Send Type-Aware Feedback to LLM -> On Success: Send Typed Enum to Exec"

This ensures that the `ActorThread` and the database only ever see validated, typed data.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### 1. Centralizing the Contract
To avoid circular dependencies between `borg-agent` (which defines the loop) and the tool-providing crates (`borg-apps`, `borg-codemode`, etc.), the exhaustive enums will live in `borg-core`.

Every crate that defines tools will move its `Args` and `Response` structs to a submodule in `borg-core` or a dedicated `borg-contracts` crate.

### 2. Refactoring Toolchain Dispatch
The `Toolchain` will be updated to dispatch based on the enum variants.

```rust
impl Toolchain<BorgToolCall, BorgToolResult> {
    pub async fn run(&self, request: ToolRequest<BorgToolCall>) -> Result<ToolResponse<BorgToolResult>> {
        match request.arguments {
            BorgToolCall::CodeModeExecuteCode(args) => {
                // Typed dispatch
            }
            // ...
        }
    }
}
```

### 3. Database Persistence
The `payload_json` in the `messages` table will now store the result of serializing the `BorgToolCall` enum. Since the enum uses a stable `tag/content` structure, history remains searchable and consistent.

## Implementation Plan
[implementation]: #implementation

### Phase 1: Core Contract
1. Create `crates/borg-core/src/tool_contract.rs`.
2. Move tool argument/response structs from `borg-exec`, `borg-apps`, `borg-codemode`, `borg-fs`, `borg-memory`, `borg-taskgraph`, and `borg-schedule`.
3. Define exhaustive `BorgToolCall` and `BorgToolResult` enums.

### Phase 2: Agent Edge Refactor
1. Update `borg-agent::Agent::run` to attempt deserialization of `ProviderBlock::ToolCall` into `BorgToolCall` immediately.
2. Implement an error handler that generates a `Message::ToolResult` with a descriptive error message when deserialization fails.

### Phase 3: Runtime Propagation
1. Update `borg-exec` to use the concrete `BorgToolCall` instead of the `Value` wrapper.
2. Update all `Tool::new_transcoded` callsites to use the new typed contract.

### Phase 4: Cleanup
1. Remove `serde_json::Value` from `BorgToolCall` and `BorgToolResult` in `borg-agent/src/tools.rs`.
2. Verify that `cargo check` no longer shows internal `Value` usage in the hot paths.

## Drawbacks
[drawbacks]: #drawbacks

- **Enum Growth**: The central enum will become large as more tools are added.
- **Boilerplate**: Adding a new tool requires updating the central contract in `borg-core`.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

We could use a `Box<dyn Any>` or keep the generic parameters, but an exhaustive enum provides the best balance of safety, performance, and "one source of truth" for the entire system's capabilities.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

- Should we use a macro to automatically register tools into the exhaustive enum?
- How do we handle dynamic tools (e.g., capabilities discovered at runtime from Apps)? These may still need a `Dynamic(Value)` catch-all variant initially.
