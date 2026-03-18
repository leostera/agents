# RFD0052 - agents-codemode

- Feature Name: `agents-codemode`
- Start Date: `2026-03-18`
- RFD PR: [leostera/borg#0001](https://github.com/leostera/borg/pull/0001)
- Borg Issue: [leostera/borg#0001](https://github.com/leostera/borg/issues/0001)

## Summary

Add a new crate, `agents-codemode`, that provides an embeddable code execution and code search engine for agent tool runners.

The crate exposes:

- `CodeMode`
- `CodeModeConfig`
- `Request`
- `Response`
- `PackageProvider`
- `EnvironmentProvider`
- `NativeFunctionRegistry`

The engine is intentionally small and typed:

- `Request::RunCode(RunCode)`
- `Request::SearchCode(SearchCode)`

It should be easy to embed in a custom `ToolRunner` by adding:

- `MyToolCall::CodeMode(agents_codemode::Request)`
- `MyToolResult::CodeMode(agents_codemode::Response)`

## Motivation

We want a reusable codemode engine that can be dropped into any agent tool surface without coupling it to one specific `ToolRunner` implementation.

The old `old-codemode` prototype proved a few important ideas:

- fresh isolate per execution is workable
- a module loader should be installed by default
- built-in host functions like `fetch` are useful
- `async () => { ... }` is a good execution contract

But its public API was too ad hoc:

- raw `ffi(op_name, args)`
- untyped string dispatch
- concrete context blobs instead of provider seams

The new crate should keep the good execution model and replace the public API with a small typed surface.

## Guide-level explanation

The authored path should look like this:

```rust
let codemode = CodeMode::builder()
    .with_config(CodeModeConfig::default().multithreaded(true))
    .with_package_provider(MyPackageProvider::new())
    .with_environment_provider(MyEnvironmentProvider::new())
    .with_native_functions(
        NativeFunctionRegistry::default()
            .add_function("read_file", read_file)
            .add_function("search_code", search_code),
    )
    .build()?;
```

Then it can be embedded directly in a custom `ToolRunner`:

```rust
match request {
    MyToolCall::CodeMode(request) => {
        let response = self.codemode.execute(request).await?;
        Ok(MyToolResult::CodeMode(response))
    }
    other => { /* ... */ }
}
```

`RunCode` executes JavaScript in a `deno_core` isolate.

Its code contract is strict:

- `RunCode.code` must be exactly an async zero-arg closure
- for example: `async () => { return { ok: true }; }`

The engine compiles that closure, calls it, awaits it, and returns the resolved value in `RunCodeResult`.

`SearchCode` does not execute JavaScript.
It searches the code made available through package providers and returns structured matches.

## Reference-level explanation

### Public API

```rust
pub struct CodeMode { ... }
pub struct CodeModeConfig { ... }

pub enum Request {
    RunCode(RunCode),
    SearchCode(SearchCode),
}

pub enum Response {
    RunCode(RunCodeResult),
    SearchCode(SearchCodeResult),
}
```

```rust
pub struct RunCode {
    pub code: String,
    pub imports: Vec<String>,
}

pub struct SearchCode {
    pub query: String,
    pub limit: Option<usize>,
}
```

```rust
pub struct RunCodeResult {
    pub value: serde_json::Value,
    pub duration: Duration,
}

pub struct SearchCodeResult {
    pub matches: Vec<PackageMatch>,
}

pub struct PackageMatch {
    pub name: String,
    pub snippet: Option<String>,
}
```

### Provider seams

```rust
pub trait PackageProvider: Send + Sync + 'static {
    async fn fetch(&self) -> anyhow::Result<Vec<Package>>;
}

pub struct Package {
    pub name: String,
    pub code: String,
}
```

```rust
pub trait EnvironmentProvider: Send + Sync + 'static {
    async fn fetch(&self) -> anyhow::Result<Vec<EnvironmentVariable>>;
}

pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}
```

Multiple providers of each kind should be supported.

### Native host functions

```rust
pub trait NativeFunction: Send + Sync + 'static {
    async fn call(&self, args: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}

pub struct NativeFunctionRegistry { ... }
```

```rust
impl NativeFunctionRegistry {
    pub fn add_function<F>(self, name: impl Into<String>, function: F) -> Self
    where
        F: NativeFunction;
}
```

These functions are exposed on the JavaScript side as async host functions.

The public API should not expose raw `ffi(op_name, args)`, even if the internal implementation uses a single dispatch bridge.

### Builder

```rust
impl CodeMode {
    pub fn builder() -> CodeModeBuilder;
    pub async fn execute(&self, request: Request) -> anyhow::Result<Response>;
}

impl CodeModeBuilder {
    pub fn with_config(self, config: CodeModeConfig) -> Self;
    pub fn with_package_provider<P>(self, provider: P) -> Self
    where
        P: PackageProvider;
    pub fn with_environment_provider<P>(self, provider: P) -> Self
    where
        P: EnvironmentProvider;
    pub fn with_native_functions(self, registry: NativeFunctionRegistry) -> Self;
    pub fn build(self) -> anyhow::Result<CodeMode>;
}
```

### Engine defaults

The engine should install by default:

- a module loader
- built-in `fetch`
- built-in environment access:
  - `env.get(key)`
  - `env.keys()`

User-provided native functions are layered on top of those defaults.

### Execution flow

For `RunCode`:

1. fetch packages from `PackageProvider`s
2. fetch environment variables from `EnvironmentProvider`s
3. create a fresh `JsRuntime`
4. install the module loader
5. install built-in host functions
6. install user-provided native functions
7. compile `(<code>)`
8. verify that it resolves to a function
9. call and await that function
10. convert the resolved value to `serde_json::Value`

For `SearchCode`:

1. fetch packages from `PackageProvider`s
2. search available code natively in Rust
3. return structured matches

## Implementation notes

The old prototype in `crates/old-codemode/` is the reference for the execution model, not the final public API.

Keep from the prototype:

- fresh isolate per execution
- module loader installation
- `async () => { ... }` execution contract
- built-in `fetch`
- host function dispatch installed before execution

Do not carry forward:

- raw public `ffi(op_name, args)`
- untyped stringly public tool surface
- concrete context blobs instead of provider traits

The internal implementation may still use a single dispatch bridge to bind host functions into `deno_core`.

## Testing plan

This crate should land with aggressive test coverage.

Unit tests:

- builder and config behavior
- package provider aggregation
- environment provider aggregation
- native function registry behavior
- search matching and limits

End-to-end tests:

- `RunCode` returning JSON values
- `RunCode` importing from provided packages
- `RunCode` reading environment values
- `RunCode` calling user native functions
- built-in `fetch`
- embedding `CodeMode` in a custom `ToolRunner`

Implementation should land in small commits with tests added alongside each slice.

## Drawbacks

- `deno_core` adds substantial implementation complexity
- module loading and import resolution can expand scope quickly
- provider traits using async methods likely require either boxed futures or `async_trait`

## Alternatives

Keep the old prototype shape with `ffi(op_name, args)`.

That is worse because:

- the public API stays stringly typed
- embedding is less ergonomic
- package/env/native seams stay implicit

Make codemode itself a `ToolRunner`.

That is worse because:

- it couples the engine to one integration path
- users still need to adapt it into their own tool enums
- the typed `Request` / `Response` boundary is already enough

## Unresolved questions

- How much of the old module loader should be reused directly, versus simplified for provider-backed imports only?
- Should built-in host functions live on globals, or under namespaces like `env.*` and `native.*`?
- Should `SearchCode` match only package names and code snippets, or also return richer location metadata in v0?
