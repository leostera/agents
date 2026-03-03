use anyhow::{Result, anyhow};
use borg_agent::{BorgToolCall, BorgToolResult, Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde::Deserialize;
use serde_json::json;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{CodeModeContext, CodeModeRuntime, sdk_types};

#[derive(Debug, Clone, Deserialize)]
struct SearchApisArgs {
    query: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExecuteCodeArgs {
    hint: String,
    code: String,
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "CodeMode-searchApis".to_string(),
            description: r#"

            Search for APIs available to execute code. ALWAYS use the `CodeMode-searchApis` tool to search for APIs before executing code. 

            Returns only APIs available in the TypeScript SDK definitions for the Borg SDK. 

            "#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "fuzzy search terms to look for in the Borg SDK"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "CodeMode-executeCode".to_string(),
            description: r#"
            Execute JavaScript in Code Mode runtime. 

            Input must be {"code": string, "hint": string} where code is exactly an async zero-arg arrow function. 

            For example: 

            ```js
            // Important to make the function async!
            async () => { 
              // You can use `await import("<registry>:<package>@<version>");`
              // to import packages dynamically, because `import * from "pkg";` syntax is not
              // allowed in this context.

              // Make sure you call the CodeMode-searchApis MCP Tool to learn what the API surface
              // is for the `Borg` global object.
              const listing = await Borg.OS.ls('.'); 

              // return a JSON-serializable object
              return listing; 
            }
            ```. 

            If you see reiceve back any syntax or compilation or runtime errors, do your best to correct the existing code snippet.

            "#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "hint": {
                        "type": "string",
                        "description": "The name of the action we're executing. For example: 'Downloading Minions movie...'"
                    },
                    "code": {
                        "type": "string",
                        "description": "JavaScript function source in the shape `async () => { ... return <json-serializable-result>; }`"
                    }
                },
                "required": ["code", "hint"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn build_code_mode_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    build_code_mode_toolchain_with_context(runtime, CodeModeContext::default())
}

pub fn build_code_mode_toolchain_with_context(
    runtime: CodeModeRuntime,
    context: CodeModeContext,
) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
    let search_spec = required_default_tool_spec("CodeMode-searchApis")?;
    let execute_spec = required_default_tool_spec("CodeMode-executeCode")?;

    Toolchain::builder()
        .add_tool(Tool::new_transcoded(
            search_spec,
            None,
            move |request: borg_agent::ToolRequest<SearchApisArgs>| async move {
                let query = request.arguments.query.trim();
                if query.is_empty() {
                    return Err(anyhow!("CodeMode-searchApis tool requires query"));
                }
                Ok(ToolResponse::<()> {
                    content: ToolResultData::Text(sdk_types().to_string()),
                })
            },
        ))?
        .add_tool(Tool::new_transcoded(
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
            move |request: borg_agent::ToolRequest<ExecuteCodeArgs>| {
                let runtime = runtime.clone();
                let context = context.clone();
                async move {
                    let _hint = request.arguments.hint;
                    let code = request.arguments.code.trim().to_string();
                    if code.is_empty() {
                        return Err(anyhow!("CodeMode-executeCode tool requires code"));
                    }
                    let result = tokio::task::spawn_blocking(move || {
                        catch_unwind(AssertUnwindSafe(|| runtime.execute(&code, context)))
                    })
                    .await
                    .map_err(|err| anyhow!("CodeMode-executeCode tool worker join error: {}", err))?
                    .map_err(|_| anyhow!("CodeMode-executeCode tool panicked"))??;
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
