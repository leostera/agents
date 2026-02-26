use anyhow::Result;
use borg_agent::Toolchain;
use borg_rt::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain, build_code_mode_toolchain_with_context};

pub fn build_exec_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain> {
    build_code_mode_toolchain(runtime)
}

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    context: CodeModeContext,
) -> Result<Toolchain> {
    build_code_mode_toolchain_with_context(runtime, context)
}
