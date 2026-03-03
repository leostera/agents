use anyhow::Result;
use borg_agent::{
    Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain, ToolchainBuilder,
    build_agent_admin_toolchain, default_agent_admin_tool_specs,
};
use borg_clockwork::build_clockwork_toolchain;
use borg_codemode::{CodeModeContext, CodeModeRuntime, build_code_mode_toolchain_with_context};
use borg_core::Uri;
use borg_db::BorgDb;
use borg_fs::{BorgFs, build_borg_fs_toolchain, default_borg_fs_tool_specs};
use borg_llm::{default_provider_admin_tool_specs, run_provider_admin_tool};
use borg_memory::{MemoryStore, build_memory_toolchain};
use borg_ports_tools::{build_port_admin_toolchain, default_port_admin_tool_specs};
use borg_shellmode::{ShellModeRuntime, build_shell_mode_toolchain};
use borg_taskgraph::{build_taskgraph_toolchain, build_taskgraph_worker_toolchain};
use serde_json::Value;

pub fn build_exec_toolchain_with_context(
    runtime: CodeModeRuntime,
    shell_runtime: ShellModeRuntime,
    context: CodeModeContext,
    memory: MemoryStore,
    db: BorgDb,
    files: BorgFs,
    current_session_id: Uri,
    current_agent_id: Uri,
    allow_task_creation: bool,
) -> Result<Toolchain<Value, Value>> {
    let code = build_code_mode_toolchain_with_context(runtime, context)?;
    let shell = build_shell_mode_toolchain(shell_runtime)?;
    let ltm = build_memory_toolchain(memory)?;
    let fs_tools = build_borg_fs_toolchain(files)?;
    let taskgraph = if allow_task_creation {
        build_taskgraph_toolchain(db.clone())?
    } else {
        build_taskgraph_worker_toolchain(db.clone())?
    };
    let clockwork = build_clockwork_toolchain(db.clone())?;
    let agent_admin =
        build_agent_admin_toolchain(db.clone(), current_session_id, current_agent_id)?;
    let port_admin = build_port_admin_toolchain(db.clone())?;
    let provider_admin = build_provider_admin_toolchain(db)?;
    code.merge(shell)?
        .merge(ltm)?
        .merge(fs_tools)?
        .merge(taskgraph)?
        .merge(clockwork)?
        .merge(agent_admin)?
        .merge(port_admin)?
        .merge(provider_admin)
}

pub fn default_exec_admin_tool_specs() -> Vec<ToolSpec> {
    let mut out = Vec::new();
    out.extend(default_agent_admin_tool_specs());
    out.extend(default_port_admin_tool_specs());
    out.extend(default_borg_fs_tool_specs());
    out.extend(
        default_provider_admin_tool_specs()
            .into_iter()
            .map(|spec| ToolSpec {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            }),
    );
    out
}

fn build_provider_admin_toolchain(db: BorgDb) -> Result<Toolchain<Value, Value>> {
    let mut builder = ToolchainBuilder::new();
    for spec in default_provider_admin_tool_specs() {
        let db = db.clone();
        let name = spec.name.clone();
        let tool = Tool::new(
            ToolSpec {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            },
            None,
            move |request| {
                let db = db.clone();
                let name = name.clone();
                async move {
                    let value = run_provider_admin_tool(&db, &name, &request.arguments).await?;
                    Ok(ToolResponse {
                        content: ToolResultData::Text(serde_json::to_string(&value)?),
                    })
                }
            },
        );
        builder = builder.add_tool(tool)?;
    }
    builder.build()
}
