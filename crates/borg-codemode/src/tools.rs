use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{CodeModeContext, CodeModeRuntime, sdk_types};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "CodeMode-searchApis".to_string(),
            description: "Search for APIs available to execute code. ALWAYS use the `CodeMode-searchApis` tool to search for APIs before executing code. Returns only APIs available in the TypeScript SDK definitions for the Borg SDK. When code needs credentials or secrets from connected apps, first discover available keys with `Borg.env.keys()` and then read specific values with `Borg.env.get('KEY', 'fallback')` (for example `Borg.env.get('APP_GITHUB_ACCESS_TOKEN')`). CodeMode supports dynamic imports inside execution payloads, for example `const kleur = (await import('npm:kleur@4.1.5')).default;` or `const semver = await import('jsr:@std/semver');`, plus local `file:` and remote `http(s):` specifiers. Package installs and resolver state are managed in `~/.borg/codemode` with `node_modules` at `~/.borg/codemode/node_modules`. Static `import ... from ...` declarations are not valid in the function payload shape, but are supported inside imported modules.".to_string(),
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
            description: "Execute JavaScript in Code Mode runtime. ALWAYS use the `CodeMode-searchApis` tool to search for APIs before executing code. Input must be {\"code\": string} where code is exactly an async zero-arg arrow function, for example: `async () => { const listing = await Borg.OS.ls('.'); return listing; }`. For app credentials/secrets, first inspect keys with `Borg.env.keys()` then fetch only needed values with `Borg.env.get(name, fallback)` (for example `const token = Borg.env.get('APP_GITHUB_ACCESS_TOKEN');`). Returns JSON from the function return value. Dynamic imports are supported via `await import(specifier)`, including package specifiers like `npm:kleur@4.1.5` and `jsr:@std/semver`, plus local `file:` and remote `http(s):` specifiers. Resolver + package install state is embedded and persisted under `~/.borg/codemode` (`~/.borg/codemode/node_modules` for installed packages). Static `import ... from ...` declarations are not supported in this function-expression payload format.".to_string(),
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

pub fn build_code_mode_toolchain(runtime: CodeModeRuntime) -> Result<Toolchain> {
    build_code_mode_toolchain_with_context(runtime, CodeModeContext::default())
}

pub fn build_code_mode_toolchain_with_context(
    runtime: CodeModeRuntime,
    context: CodeModeContext,
) -> Result<Toolchain> {
    let search_spec = required_default_tool_spec("CodeMode-searchApis")?;
    let execute_spec = required_default_tool_spec("CodeMode-executeCode")?;

    Toolchain::builder()
        .add_tool(Tool::new(search_spec, None, move |request| async move {
            let query = request
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("CodeMode-searchApis tool requires query"))?;
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
                let context = context.clone();
                async move {
                    let code = request
                        .arguments
                        .get("code")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("CodeMode-executeCode tool requires code"))?;
                    let code = code.to_string();
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
