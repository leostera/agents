use anyhow::Result;
use borg_agent::Toolchain;
use borg_codemode::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain_with_context};
use borg_ltm::{MemoryStore, build_memory_toolchain};

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
) -> Result<Toolchain> {
    let code = build_code_mode_toolchain_with_context(runtime, context)?;
    let ltm = build_memory_toolchain(memory)?;
    code.merge(ltm)
}
