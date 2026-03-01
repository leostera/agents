use anyhow::Result;
use borg_agent::Toolchain;
use borg_codemode::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain_with_context};
use borg_core::Uri;
use borg_db::BorgDb;
use borg_memory::{MemoryStore, build_memory_toolchain};
use borg_shellmode::{ShellModeRuntime, build_shell_mode_toolchain};
use borg_taskgraph::{build_taskgraph_toolchain, build_taskgraph_worker_toolchain};

use crate::admin_tools::build_admin_toolchain;

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    shell_runtime: ShellModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
    db: BorgDb,
    current_session_id: Uri,
    current_agent_id: Uri,
    allow_task_creation: bool,
) -> Result<Toolchain> {
    let code = build_code_mode_toolchain_with_context(runtime, context)?;
    let shell = build_shell_mode_toolchain(shell_runtime)?;
    let ltm = build_memory_toolchain(memory)?;
    let taskgraph = if allow_task_creation {
        build_taskgraph_toolchain(db.clone())?
    } else {
        build_taskgraph_worker_toolchain(db.clone())?
    };
    let admin = build_admin_toolchain(db, current_session_id, current_agent_id)?;
    code.merge(shell)?
        .merge(ltm)?
        .merge(taskgraph)?
        .merge(admin)
}
