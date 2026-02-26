use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::json;

use crate::{FactInput, MemoryStore, SearchQuery};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "saveFacts".to_string(),
            description: "Persist long-term memory facts. Input must be {\"facts\": FactInput[]}.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "facts": {
                        "type": "array",
                        "description": "List of facts to persist in LTM"
                    }
                },
                "required": ["facts"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "searchMemory".to_string(),
            description: "Search long-term memory entities. Input must be {\"query\": SearchQuery}.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "object",
                        "description": "Search query payload for LTM"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn build_memory_toolchain(memory: MemoryStore) -> Result<Toolchain> {
    let save_facts_spec = required_default_tool_spec("saveFacts")?;
    let search_memory_spec = required_default_tool_spec("searchMemory")?;
    let memory_for_save = memory.clone();
    let memory_for_search = memory;

    Toolchain::builder()
        .add_tool(Tool::new(save_facts_spec, None, move |request| {
            let memory = memory_for_save.clone();
            async move {
                let facts_value = request
                    .arguments
                    .get("facts")
                    .cloned()
                    .ok_or_else(|| anyhow!("saveFacts tool requires facts"))?;
                let facts: Vec<FactInput> = serde_json::from_value(facts_value)?;
                if facts.is_empty() {
                    return Err(anyhow!("saveFacts expects a non-empty facts array"));
                }

                let result = memory.state_facts(facts).await?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&result)?),
                })
            }
        }))?
        .add_tool(Tool::new(search_memory_spec, None, move |request| {
            let memory = memory_for_search.clone();
            async move {
                let query_value = request
                    .arguments
                    .get("query")
                    .cloned()
                    .ok_or_else(|| anyhow!("searchMemory tool requires query"))?;
                let query: SearchQuery = serde_json::from_value(query_value)?;
                let results = memory.search_query(query).await?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&results)?),
                })
            }
        }))?
        .build()
}

fn required_default_tool_spec(name: &str) -> Result<ToolSpec> {
    default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| anyhow!("missing default tool spec `{}`", name))
}
