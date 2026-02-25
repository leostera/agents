use anyhow::{Result, anyhow};
use borg_agent::{
    CapabilitySummary, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain, default_tool_specs,
};
use borg_rt::CodeModeRuntime;
use serde_json::{Value, json};

pub fn build_exec_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain> {
    let search_spec = required_default_tool_spec("search")?;
    let execute_spec = required_default_tool_spec("execute")?;

    Toolchain::builder()
        .add_tool(Tool::new(search_spec, None, {
            let runtime = runtime.clone();
            move |request| {
                let runtime = runtime.clone();
                async move {
                    let query = request
                        .arguments
                        .get("query")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("search tool requires query"))?;
                    let results = runtime
                        .search(query)
                        .into_iter()
                        .map(|api| CapabilitySummary {
                            name: api.name,
                            signature: api.signature,
                            description: api.description,
                        })
                        .collect();
                    Ok(ToolResponse {
                        content: ToolResultData::Capabilities(results),
                    })
                }
            }
        }))?
        .add_tool(Tool::new(
            execute_spec,
            Some(json!({
                "type": "object",
                "properties": {
                    "Execution": {
                        "type": "object",
                        "properties": {
                            "result": {},
                            "duration_ms": { "type": "number" }
                        },
                        "required": ["result", "duration_ms"],
                        "additionalProperties": false
                    }
                },
                "required": ["Execution"],
                "additionalProperties": false
            })),
            move |request| {
                let runtime = runtime.clone();
                async move {
                    let code = request
                        .arguments
                        .get("code")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("execute tool requires code"))?;
                    let result = runtime.execute(code)?;
                    Ok(ToolResponse {
                        content: ToolResultData::Execution {
                            result: result.result_json,
                            duration_ms: result.duration_ms,
                        },
                    })
                }
            },
        ))?
        .build()
}

fn required_default_tool_spec(name: &str) -> Result<ToolSpec> {
    default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| anyhow!("missing default tool spec `{}`", name))
}
