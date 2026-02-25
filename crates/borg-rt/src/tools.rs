use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::{Value, json};

use crate::{CodeModeRuntime, sdk_types};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "search".to_string(),
            description: "Return the full TypeScript SDK definitions from borg.d.ts so the model can inspect available APIs and their exact types. Input must be {\"query\": string}; query is currently ignored.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Currently ignored. Reserved for future targeted type search."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "execute".to_string(),
            description: "Execute JavaScript in Code Mode runtime. Input must be {\"code\": string} where code is exactly an async zero-arg arrow function, for example: async () => { const x = await Borg.fetch(...); return x; }. Returns JSON from the function return value.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "JavaScript function source in the shape `async () => { ... return <json-serializable-result>; }`"
                    }
                },
                "required": ["code"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn build_code_mode_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain> {
    let search_spec = required_default_tool_spec("search")?;
    let execute_spec = required_default_tool_spec("execute")?;

    Toolchain::builder()
        .add_tool(Tool::new(search_spec, None, move |request| async move {
            let query = request
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("search tool requires query"))?;
            let _ = query;
            Ok(ToolResponse {
                content: ToolResultData::Text(sdk_types().to_string()),
            })
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
                            "duration": {
                                "type": "object",
                                "properties": {
                                    "secs": { "type": "number" },
                                    "nanos": { "type": "number" }
                                },
                                "required": ["secs", "nanos"],
                                "additionalProperties": false
                            }
                        },
                        "required": ["result", "duration"],
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
                            duration: result.duration,
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
