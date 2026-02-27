use anyhow::Result;
use borg_agent::Toolchain;
use borg_ltm::{MemoryStore, build_memory_toolchain};
use borg_rt::{
    CodeModeContext, CodeModeRuntime, build_code_mode_toolchain,
    build_code_mode_toolchain_with_context,
};

pub fn build_exec_toolchain(runtime: CodeModeRuntime, memory: MemoryStore) -> Result<Toolchain> {
    let code = build_code_mode_toolchain(runtime)?;
    let ltm = build_memory_toolchain(memory)?;
    code.merge(ltm)
}

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
) -> Result<Toolchain> {
    let code = build_code_mode_toolchain_with_context(runtime, context)?;
    let ltm = build_memory_toolchain(memory)?;
    code.merge(ltm)
}
