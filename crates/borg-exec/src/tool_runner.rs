use anyhow::Result;
use borg_agent::Toolchain;
use borg_rt::{CodeModeRuntime, build_code_mode_toolchain};

pub fn build_exec_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain> {
    build_code_mode_toolchain(runtime)
}
