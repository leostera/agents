use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::json;

use crate::{FactInput, MemoryStore, SearchQuery};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "saveFacts".to_string(),
            description: r#"
This tool lets you persist information into Borg's long-term memory as durable facts.

When to use:
- The user shares a stable personal fact or preference worth remembering.
- The user gives reusable environment details (paths, services, IDs, conventions).
- You infer a useful, low-risk fact from explicit user statements.

Why use it:
- Future turns can recall this information via `searchMemory`.
- Facts are durable across sessions and improve personalization.
- Saving in batches reduces overhead and keeps memory writes coherent.

How to use it well:
- Save facts eagerly and granularly.
- Prefer multiple small facts in one call over one large opaque blob.
- Use canonical URIs for `source`, `entity`, and `field`.
- Include `source` whenever you have message provenance.

Input shape:
{ "facts": FactInput[] }

Examples:
{
  "facts": [
    {
      "source": "borg:message:telegram_2654566_13842",
      "entity": "borg:user:leostera",
      "field": "borg:field:full_name",
      "value": { "Text": "Leandro Ostera Villalva" }
    },
    {
      "source": "borg:message:telegram_2654566_13842",
      "entity": "borg:user:leostera",
      "field": "borg:field:nickname",
      "value": { "Text": "Leo" }
    }
  ]
}

{
  "facts": [
    {
      "entity": "borg:user:leostera",
      "field": "borg:preference:favorite_movie",
      "value": { "Text": "Minions" }
    }
  ]
}
"#.to_string(),
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
            description: r#"
This tool searches Borg's long-term memory for previously saved entities and facts.

When to use:
- Before answering questions about user preferences, profile, or prior context.
- Before asking the user to repeat information that might already be known.
- Before deciding to write new facts, to avoid duplication.

Why use it:
- Improves answer quality with grounded recalled data.
- Reduces unnecessary follow-up questions.
- Keeps conversations consistent across sessions.

How to use it well:
- Start with a focused `q` query and sensible `limit`.
- Add `ns`/`kind` filters when you need narrower results.
- Use `name.like` when you have partial names.

Input shape:
{ "query": SearchQuery }

Examples:
{
  "query": {
    "q": "girlfriend",
    "limit": 10
  }
}

{
  "query": {
    "ns": "borg",
    "kind": "user",
    "name": { "like": "leo" },
    "limit": 5
  }
}
"#.to_string(),
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
