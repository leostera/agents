# RFD0008 - Shell Mode: Arbitrary Shell Command Execution

- Feature Name: `shell_mode_execution`
- Start Date: `2026-03-01`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary

We introduce **Shell Mode** as a second execution runtime for Borg agents, complementing Code Mode. While Code Mode executes JavaScript in a sandboxed Deno runtime, Shell Mode executes arbitrary shell commands on the host system. This enables agents to perform CLI-oriented operations that are cumbersome or impractical in JavaScript.

## Guide-level explanation

Shell Mode exposes a single MCP tool that allows agents to execute shell commands with configurable timeout and working directory.

### Mental model

Shell Mode is to CLI what Code Mode is to JavaScript. It provides direct access to the host system's shell, enabling operations like:

- File inspection: `ls`, `cat`, `find`, `grep`
- Process management: `ps`, `kill`, `top`
- Version control: `git` operations
- Build tools: `cargo`, `npm`, `make`
- System info: `uname`, `df`, `free`

### Tool interface

```
ShellMode-executeCommand({
  command: string,      // The shell command to execute
  hint: string,         // Human-readable description of the action
  timeout_seconds?: number,  // Optional timeout override (default: 30s)
  working_directory?: string // Optional working directory override
})
```

### Response format

```json
{
  "stdout": "command output...",
  "stderr": "error output...",
  "exit_code": 0,
  "duration": { "secs": 0, "nanos": 123456789 }
}
```

## Reference-level explanation

### Crate: `borg-shellmode`

Location: `crates/borg-shellmode/`

#### Module structure

```
src/
├── lib.rs        # Public exports
├── tools.rs      # ToolSpec definitions and Toolchain builder
├── engine.rs     # Command execution logic
└── types.rs      # Shared types
```

#### Public API

```rust
// ShellModeRuntime: the execution engine
pub struct ShellModeRuntime { ... }
impl ShellModeRuntime {
    pub fn new() -> Self
    pub fn with_default_timeout(mut self, timeout: Duration) -> Self
    pub fn with_working_directory(mut self, dir: PathBuf) -> Self
    pub fn execute(&self, command: &str, ctx: ShellModeContext) -> Result<ExecutionResult>
}

// ShellModeContext: per-invocation context
#[derive(Clone, Default)]
pub struct ShellModeContext {
    pub working_directory: Option<PathBuf>,
    pub timeout_seconds: Option<u64>,
}

// Toolchain builder
pub fn build_shell_mode_toolchain(runtime: ShellModeRuntime) -> Result<Toolchain>
pub fn default_tool_specs() -> Vec<ToolSpec>
```

#### Execution flow

1. Parse tool arguments (command, hint, optional timeout, optional cwd)
2. Resolve working directory (context override → runtime default → current dir)
3. Execute command via `std::process::Command` with configured timeout
4. Capture stdout, stderr, exit code
5. Return structured result

#### Security model

Shell Mode is intentionally unrestricted - it executes whatever command the agent provides. This is by design: it provides maximum flexibility for CLI operations. Access should be controlled at a higher layer (agent grants, capability grants per RFD0004) rather than within the runtime itself.

### Relationship to RFD0004

Shell Mode implements the `shell` execution mode referenced in the capabilities table. A capability with `execution_mode: "shell"` carries an `execution_spec_json` that can specify:

- Command template
- Default timeout
- Allowed working directories
- Environment variable constraints (future)

This crate provides the runtime substrate; the capability layer provides the policy.

## Drawbacks

1. **Security**: Arbitrary shell execution is inherently risky. Misuse can lead to data loss, system compromise, or unintended side effects. This must be mitigated at the agent/capability granting layer, not within this crate.

2. **Platform dependency**: Shell commands are inherently OS-specific. Commands that work on Linux may fail on macOS or Windows.

3. **Output size**: Large command outputs can consume significant memory. Consider adding output truncation for production use.

4. **Blocking**: Long-running commands block the execution thread. Consider async execution for production use.

## Rationale and alternatives

We considered:
- **Allowlisted commands only**: Rejected because it defeats the purpose of shell access for flexible CLI operations
- **Sandboxed execution (容器/VMs)**: Deferred for future work; adds significant complexity
- **Interactive shell sessions**: Deferred; single-command model is simpler and sufficient for most use cases

## Prior art

- **MCP `execute_command`**: Model Context Protocol defines a command execution capability
- **OpenAI Code Interpreter**: Provides shell access in sandboxed environments
- **GitHub Actions `run`**: Shell execution in CI/CD context

## Future possibilities

1. **Output truncation**: Limit stdout/stderr size to prevent memory issues
2. **Async execution**: Run commands asynchronously for better concurrency
3. **Environment filtering**: Allow specifying which env vars to pass
4. **Shell session state**: Support persistent shell sessions for multi-command workflows
5. **Streaming output**: Return output as it's generated rather than all at once
