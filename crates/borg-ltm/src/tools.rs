use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::json;
use std::collections::HashSet;

use crate::{FactInput, MemoryStore, SearchQuery, Uri};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "newEntity".to_string(),
            description: r#"
Create a new entity URI when you cannot confidently find an existing one.

When to use:
- After a `searchMemory` lookup fails to find a reliable match.
- Before saving facts about a concrete entity (person, movie, place, thing)
  that should be referenceable later.

Workflow:
1) Try `searchMemory` first.
2) If no strong match exists, call `newEntity`.
3) Save intrinsic facts on that new URI via `saveFacts`.
4) Link other entities to it using `Ref`.

Input shape:
{ "ns": string, "kind": string }

Example:
{ "ns": "borg", "kind": "person" }
"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ns": { "type": "string", "description": "Entity namespace, e.g. borg" },
                    "kind": { "type": "string", "description": "Entity kind, e.g. person, movie" }
                },
                "required": ["ns", "kind"],
                "additionalProperties": false
            }),
        },
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
- Always attempt `searchMemory` first for likely existing entities.
- If no reliable match exists, create an entity via `newEntity` first.
- Save facts eagerly and granularly.
- Prefer multiple small facts in one call over one large opaque blob.
- Use canonical URIs for `source`, `entity`, and `field`.
- Include `source` whenever you have message provenance.
- Prefer creating first-class entity URIs and linking via `Ref` instead of embedding related attributes on the same entity.
- Set `arity` explicitly when a field can hold multiple values.
  - `one` (default): single-valued field, latest fact overwrites projection.
  - `many`: multi-valued field, projection stores a deduplicated array.

Entity modeling preference:
- For new concrete entities (people, places, things), create a dedicated URI (for example `borg:person:<uuid>`).
- Attach intrinsic attributes (`name`, `nickname`, etc.) to that entity.
- Link users/things to that entity using relationship facts with `Ref`.
- This keeps memory graph-structured and referenceable across future facts.

`value` variants (single-key object):
- Text: `{ "Text": "Leo" }`
- Integer: `{ "Integer": 42 }`
- Float: `{ "Float": 3.14 }`
- Boolean: `{ "Boolean": true }`
- Bytes: `{ "Bytes": [137, 80, 78, 71] }`
- Ref: `{ "Ref": "borg:user:mariana" }`

Input shape:
{ "entities": string[], "facts": FactInput[] }

`entities` is REQUIRED and must list every entity URI that appears in `facts[*].entity`.
This enforces entity-first memory modeling:
- first discover entities via `searchMemory` (or create with `newEntity`)
- then state facts only for that explicit entity set
- this guarantees people/places/objects get stable, referenceable entity nodes

Examples:
{
  "entities": ["borg:user:leostera"],
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
      "arity": "one",
      "value": { "Text": "Leo" }
    }
  ]
}

{
  "entities": ["borg:user:leostera"],
  "facts": [
    {
      "entity": "borg:user:leostera",
      "field": "borg:preference:hobby",
      "arity": "many",
      "value": { "Text": "climbing" }
    },
    {
      "entity": "borg:user:leostera",
      "field": "borg:preference:hobby",
      "arity": "many",
      "value": { "Text": "cooking" }
    }
  ]
}

{
  "entities": ["borg:user:leostera", "borg:file:avatar", "borg:user:mariana"],
  "facts": [
    {
      "source": "borg:message:telegram_2654566_13843",
      "entity": "borg:user:leostera",
      "field": "borg:field:age",
      "value": { "Integer": 31 }
    },
    {
      "source": "borg:message:telegram_2654566_13843",
      "entity": "borg:user:leostera",
      "field": "borg:field:height_m",
      "value": { "Float": 1.78 }
    },
    {
      "source": "borg:message:telegram_2654566_13843",
      "entity": "borg:user:leostera",
      "field": "borg:preference:vegan",
      "value": { "Boolean": false }
    },
    {
      "source": "borg:message:telegram_2654566_13843",
      "entity": "borg:file:avatar",
      "field": "borg:field:signature",
      "value": { "Bytes": [1, 2, 3, 4] }
    },
    {
      "source": "borg:message:telegram_2654566_13843",
      "entity": "borg:user:leostera",
      "field": "borg:relationship:girlfriend",
      "value": { "Ref": "borg:user:mariana" }
    }
  ]
}

Entity-first example (preferred):
User says: "my girlfriend's name is Mariana but her nickname is Maja"

{
  "entities": [
    "borg:person:2a7f8f3b-1b11-4ef7-a1b0-9a3c2d4e5f6a",
    "borg:user:leostera"
  ],
  "facts": [
    {
      "source": "borg:message:telegram_2654566_13844",
      "entity": "borg:person:2a7f8f3b-1b11-4ef7-a1b0-9a3c2d4e5f6a",
      "field": "borg:field:name",
      "value": { "Text": "Mariana" }
    },
    {
      "source": "borg:message:telegram_2654566_13844",
      "entity": "borg:person:2a7f8f3b-1b11-4ef7-a1b0-9a3c2d4e5f6a",
      "field": "borg:field:nickname",
      "value": { "Text": "Maja" }
    },
    {
      "source": "borg:message:telegram_2654566_13844",
      "entity": "borg:user:leostera",
      "field": "borg:relationship:partnerOf",
      "value": { "Ref": "borg:person:2a7f8f3b-1b11-4ef7-a1b0-9a3c2d4e5f6a" }
    }
  ]
}
"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entities": {
                        "type": "array",
                        "description": "Required set of entity URIs this batch is allowed to state facts about",
                        "items": { "type": "string" },
                        "minItems": 1
                    },
                    "facts": {
                        "type": "array",
                        "description": "List of facts to persist in LTM",
                        "items": {
                            "type": "object",
                            "properties": {
                                "source": {
                                    "type": "string",
                                    "description": "URI identifying provenance for this fact"
                                },
                                "entity": {
                                    "type": "string",
                                    "description": "Subject URI receiving this fact"
                                },
                                "field": {
                                    "type": "string",
                                    "description": "Field URI describing the property"
                                },
                                "arity": {
                                    "type": "string",
                                    "enum": ["one", "many"],
                                    "description": "Field cardinality in the projection: one (default overwrite) or many (deduplicated array append)"
                                },
                                "value": {
                                    "description": "Fact value encoded as a single-key object variant",
                                    "oneOf": [
                                        {
                                            "type": "object",
                                            "properties": { "Text": { "type": "string" } },
                                            "required": ["Text"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": { "Integer": { "type": "integer" } },
                                            "required": ["Integer"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": { "Float": { "type": "number" } },
                                            "required": ["Float"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": { "Boolean": { "type": "boolean" } },
                                            "required": ["Boolean"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": {
                                                "Bytes": {
                                                    "type": "array",
                                                    "items": { "type": "integer" }
                                                }
                                            },
                                            "required": ["Bytes"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": { "Ref": { "type": "string" } },
                                            "required": ["Ref"],
                                            "additionalProperties": false
                                        }
                                    ]
                                }
                            },
                            "required": ["source", "entity", "field", "value"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["entities", "facts"],
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

`query` is an Apache Lucene-like query object (filters + text query fields),
adapted to Borg's memory schema (`q`, `ns`, `kind`, `name.like`, `limit`).

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
    let new_entity_spec = required_default_tool_spec("newEntity")?;
    let save_facts_spec = required_default_tool_spec("saveFacts")?;
    let search_memory_spec = required_default_tool_spec("searchMemory")?;
    let memory_for_save = memory.clone();
    let memory_for_search = memory;

    Toolchain::builder()
        .add_tool(Tool::new(
            new_entity_spec,
            None,
            move |request| async move {
                let ns = request
                    .arguments
                    .get("ns")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow!("newEntity tool requires ns"))?;
                let kind = request
                    .arguments
                    .get("kind")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow!("newEntity tool requires kind"))?;
                let entity = Uri::from_parts(ns, kind, Some(&uuid::Uuid::now_v7().to_string()))?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(entity.to_string()),
                })
            },
        ))?
        .add_tool(Tool::new(save_facts_spec, None, move |request| {
            let memory = memory_for_save.clone();
            async move {
                let entities_value = request
                    .arguments
                    .get("entities")
                    .cloned()
                    .ok_or_else(|| anyhow!("saveFacts tool requires entities"))?;
                let entities: Vec<String> = serde_json::from_value(entities_value)?;
                if entities.is_empty() {
                    return Err(anyhow!("saveFacts expects a non-empty entities array"));
                }
                let declared_entities: HashSet<String> = entities.into_iter().collect();
                let facts_value = request
                    .arguments
                    .get("facts")
                    .cloned()
                    .ok_or_else(|| anyhow!("saveFacts tool requires facts"))?;
                let facts: Vec<FactInput> = serde_json::from_value(facts_value)?;
                if facts.is_empty() {
                    return Err(anyhow!("saveFacts expects a non-empty facts array"));
                }
                if let Some(missing) = facts
                    .iter()
                    .map(|fact| fact.entity.to_string())
                    .find(|entity| !declared_entities.contains(entity))
                {
                    return Err(anyhow!(
                        "saveFacts facts contain undeclared entity `{}`; include it in `entities`",
                        missing
                    ));
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
