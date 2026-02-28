use anyhow::{Result, anyhow};
use borg_agent::{Tool, ToolResponse, ToolResultData, ToolSpec, Toolchain};
use serde_json::{Value, json};
use std::collections::HashSet;

use crate::{FactInput, FactValue, MemoryStore, SearchQuery, Uri};

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "Memory-stateFacts".to_string(),
            description: "Write a batch of memory facts. This call is the transaction boundary and returns txId + factUris.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "facts": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "entity": { "type": "string" },
                                "field": { "type": "string" },
                                "value": {}
                            },
                            "required": ["entity", "field", "value"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["source", "facts"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-search".to_string(),
            description: "Fuzzy-search memory entities and schema objects.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "resultTypes": { "type": "array", "items": { "type": "string" } },
                    "namespacePrefixes": { "type": "array", "items": { "type": "string" } },
                    "kindUris": { "type": "array", "items": { "type": "string" } },
                    "fieldUris": { "type": "array", "items": { "type": "string" } },
                    "pagination": {
                        "type": "object",
                        "properties": {
                            "limit": { "type": "integer", "minimum": 1 }
                        },
                        "additionalProperties": true
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-createEntity".to_string(),
            description: "Create an entity URI for the given kind and optionally set display label.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "kindUri": { "type": "string" },
                    "entityUri": { "type": "string" },
                    "label": { "type": "string" },
                    "source": { "type": "string" }
                },
                "required": ["kindUri", "source"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-getEntity".to_string(),
            description: "Get an entity by URI and return a read payload with warnings.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entityUri": { "type": "string" },
                    "includeRetracted": { "type": "boolean", "default": false },
                    "factPagination": {
                        "type": "object",
                        "properties": {
                            "limit": { "type": "integer", "minimum": 1 }
                        },
                        "additionalProperties": true
                    }
                },
                "required": ["entityUri"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-retractFacts".to_string(),
            description: "Retract facts by factUri or exact pattern. Placeholder until full retraction semantics are implemented.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "targets": { "type": "array", "minItems": 1 }
                },
                "required": ["source", "targets"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-listFacts".to_string(),
            description: "List facts with filters and include warnings payload.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "field": { "type": "string" },
                    "includeRetracted": { "type": "boolean", "default": false },
                    "pagination": {
                        "type": "object",
                        "properties": {
                            "limit": { "type": "integer", "minimum": 1 }
                        },
                        "additionalProperties": true
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-Schema-defineNamespace".to_string(),
            description: "Define a namespace schema entity. Placeholder until full schema writer is implemented.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "namespaceUri": { "type": "string" },
                    "prefix": { "type": "string" },
                    "source": { "type": "string" }
                },
                "required": ["namespaceUri", "prefix", "source"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-Schema-defineKind".to_string(),
            description: "Define a kind schema entity. Placeholder until full schema writer is implemented.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "kindUri": { "type": "string" },
                    "source": { "type": "string" }
                },
                "required": ["kindUri", "source"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-Schema-defineField".to_string(),
            description: "Define a field schema entity. Placeholder until full schema writer is implemented.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "fieldUri": { "type": "string" },
                    "domain": { "type": "array", "items": { "type": "string" } },
                    "range": { "type": "array", "items": { "type": "string" } },
                    "allowsMany": { "type": "boolean" },
                    "source": { "type": "string" }
                },
                "required": ["fieldUri", "domain", "range", "allowsMany", "source"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-getSchema".to_string(),
            description: r#"
Get the baseline memory schema for entities and facts. This tool takes no arguments.

STRONGLY RECOMMENDED: call `Memory-getSchema` before using any other memory tool so you know which
fields, kinds, value variants, and shapes are expected.
"#
            .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-newEntity".to_string(),
            description: r#"
Create a new entity URI when you cannot confidently find an existing one.

STRONGLY RECOMMENDED: call `Memory-getSchema` first to confirm allowed namespaces, kinds, and fields.

When to use:
- After a `Memory-searchMemory` lookup fails to find a reliable match.
- Before saving facts about a concrete entity (person, movie, place, thing)
  that should be referenceable later.

Workflow:
1) Try `Memory-searchMemory` first.
2) If no strong match exists, call `Memory-newEntity`.
3) Save intrinsic facts on that new URI via `Memory-saveFacts`.
4) Link other entities to it using `Ref`.

Input shape:
{ "ns": string, "kind": string, "displayName": string }

Example:
{ "ns": "borg", "kind": "person", "displayName": "Mariana" }
"#.to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ns": { "type": "string", "description": "Entity namespace, e.g. borg" },
                    "kind": { "type": "string", "description": "Entity kind, e.g. person, movie" },
                    "displayName": { "type": "string", "description": "Human readable name persisted to borg:field:displayName" }
                },
                "required": ["ns", "kind", "displayName"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Memory-saveFacts".to_string(),
            description: r#"
This tool lets you persist information into Borg's long-term memory as durable facts.

STRONGLY RECOMMENDED: call `Memory-getSchema` first to confirm valid fields, value variants, and arity.

When to use:
- The user shares a stable personal fact or preference worth remembering.
- The user gives reusable environment details (paths, services, IDs, conventions).
- You infer a useful, low-risk fact from explicit user statements.

Why use it:
- Future turns can recall this information via `Memory-searchMemory`.
- Facts are durable across sessions and improve personalization.
- Saving in batches reduces overhead and keeps memory writes coherent.

How to use it well:
- Always attempt `Memory-searchMemory` first for likely existing entities.
- If no reliable match exists, create an entity via `Memory-newEntity` first.
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
- first discover entities via `Memory-searchMemory` (or create with `Memory-newEntity`)
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
            name: "Memory-searchMemory".to_string(),
            description: r#"
This tool searches Borg's long-term memory for previously saved entities and facts.

STRONGLY RECOMMENDED: call `Memory-getSchema` first so your query terms align with the known schema.

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
    let state_facts_spec = required_default_tool_spec("Memory-stateFacts")?;
    let search_spec = required_default_tool_spec("Memory-search")?;
    let create_entity_spec = required_default_tool_spec("Memory-createEntity")?;
    let get_entity_spec = required_default_tool_spec("Memory-getEntity")?;
    let retract_facts_spec = required_default_tool_spec("Memory-retractFacts")?;
    let list_facts_spec = required_default_tool_spec("Memory-listFacts")?;
    let define_namespace_spec = required_default_tool_spec("Memory-Schema-defineNamespace")?;
    let define_kind_spec = required_default_tool_spec("Memory-Schema-defineKind")?;
    let define_field_spec = required_default_tool_spec("Memory-Schema-defineField")?;
    let get_schema_spec = required_default_tool_spec("Memory-getSchema")?;
    let new_entity_spec = required_default_tool_spec("Memory-newEntity")?;
    let save_facts_spec = required_default_tool_spec("Memory-saveFacts")?;
    let search_memory_spec = required_default_tool_spec("Memory-searchMemory")?;
    let memory_for_state_facts = memory.clone();
    let memory_for_search_v2 = memory.clone();
    let memory_for_create_entity = memory.clone();
    let memory_for_define_namespace = memory.clone();
    let memory_for_define_kind = memory.clone();
    let memory_for_define_field = memory.clone();
    let memory_for_get_entity = memory.clone();
    let memory_for_retract = memory.clone();
    let memory_for_list = memory.clone();
    let memory_for_new_entity = memory.clone();
    let memory_for_save = memory.clone();
    let memory_for_search = memory;

    Toolchain::builder()
        .add_tool(Tool::new(state_facts_spec, None, move |request| {
            let memory = memory_for_state_facts.clone();
            async move {
                let source = request
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-stateFacts tool requires source"))?;
                let source_uri = Uri::parse(source)?;
                let facts = request
                    .arguments
                    .get("facts")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("Memory-stateFacts tool requires facts"))?;
                if facts.is_empty() {
                    return Err(anyhow!("Memory-stateFacts expects a non-empty facts array"));
                }

                let mut inputs = Vec::with_capacity(facts.len());
                for fact in facts {
                    let entity = fact
                        .get("entity")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("Memory-stateFacts fact requires entity"))?;
                    let field = fact
                        .get("field")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("Memory-stateFacts fact requires field"))?;
                    let value = fact
                        .get("value")
                        .ok_or_else(|| anyhow!("Memory-stateFacts fact requires value"))?;
                    inputs.push(FactInput {
                        source: source_uri.clone(),
                        entity: Uri::parse(entity)?,
                        field: Uri::parse(field)?,
                        arity: Default::default(),
                        value: parse_rfd_value(value)?,
                    });
                }
                let result = memory.state_facts(inputs).await?;
                let out = json!({
                    "txId": result.tx_id.to_string(),
                    "factUris": result.facts.iter().map(|fact| fact.fact_id.to_string()).collect::<Vec<_>>(),
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(search_spec, None, move |request| {
            let memory = memory_for_search_v2.clone();
            async move {
                let query = request
                    .arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-search tool requires query"))?;
                let limit = request
                    .arguments
                    .get("pagination")
                    .and_then(|v| v.get("limit"))
                    .and_then(Value::as_u64)
                    .map(|v| v as usize)
                    .or_else(|| {
                        request
                            .arguments
                            .get("limit")
                            .and_then(Value::as_u64)
                            .map(|v| v as usize)
                    })
                    .unwrap_or(25);
                let results = memory
                    .search_query(SearchQuery {
                        ns: None,
                        kind: None,
                        name: None,
                        query_text: Some(query.to_string()),
                        limit: Some(limit),
                    })
                    .await?;
                let out = json!({
                    "results": results.entities.iter().map(|entity| {
                        json!({
                            "type": "entity",
                            "uri": entity.entity_id,
                            "score": 1.0,
                            "highlights": [],
                            "snippetFacts": []
                        })
                    }).collect::<Vec<_>>()
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(create_entity_spec, None, move |request| {
            let memory = memory_for_create_entity.clone();
            async move {
                let kind_uri = request
                    .arguments
                    .get("kindUri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-createEntity tool requires kindUri"))?;
                let source = request
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-createEntity tool requires source"))?;
                let source_uri = Uri::parse(source)?;
                let label = request
                    .arguments
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("entity");
                let mut parts = kind_uri.split(':');
                let ns = parts
                    .next()
                    .ok_or_else(|| anyhow!("invalid kindUri `{}`", kind_uri))?;
                let kind_marker = parts
                    .next()
                    .ok_or_else(|| anyhow!("invalid kindUri `{}`", kind_uri))?;
                let kind = parts
                    .next()
                    .ok_or_else(|| anyhow!("invalid kindUri `{}`", kind_uri))?;
                if kind_marker != "kind" {
                    return Err(anyhow!(
                        "kindUri must be in the form ns:kind:<id>, got `{}`",
                        kind_uri
                    ));
                }
                let entity = if let Some(entity_uri) =
                    request.arguments.get("entityUri").and_then(Value::as_str)
                {
                    Uri::parse(entity_uri)?
                } else {
                    Uri::from_parts(ns, kind, Some(&uuid::Uuid::now_v7().to_string()))?
                };
                let facts = vec![
                    FactInput {
                        source: source_uri.clone(),
                        entity: entity.clone(),
                        field: Uri::parse("borg:field:type")?,
                        arity: Default::default(),
                        value: FactValue::Ref(Uri::parse(kind_uri)?),
                    },
                    FactInput {
                        source: source_uri,
                        entity: entity.clone(),
                        field: Uri::parse("borg:field:label")?,
                        arity: Default::default(),
                        value: FactValue::Text(label.to_string()),
                    },
                ];
                let result = memory.state_facts(facts).await?;
                let out = json!({
                    "entityUri": entity.to_string(),
                    "txId": result.tx_id.to_string(),
                    "facts": result.facts.iter().map(|fact| fact.fact_id.to_string()).collect::<Vec<_>>()
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(get_entity_spec, None, move |request| {
            let memory = memory_for_get_entity.clone();
            async move {
                let entity_uri = request
                    .arguments
                    .get("entityUri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-getEntity tool requires entityUri"))?;
                let entity_uri = Uri::parse(entity_uri)?;
                let entity = memory.get_entity_uri(&entity_uri).await?;
                let out = json!({
                    "entityUri": entity_uri.to_string(),
                    "entity": entity,
                    "facts": [],
                    "warnings": []
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(retract_facts_spec, None, move |request| {
            let memory = memory_for_retract.clone();
            async move {
                let source = request
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-retractFacts tool requires source"))?;
                let source_uri = Uri::parse(source)?;
                let targets = request
                    .arguments
                    .get("targets")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("Memory-retractFacts tool requires targets"))?;
                if targets.is_empty() {
                    return Err(anyhow!("Memory-retractFacts expects a non-empty targets array"));
                }

                let mut retract_ids: Vec<String> = Vec::new();
                for target in targets {
                    if let Some(fact_uri) = target.get("factUri").and_then(Value::as_str) {
                        retract_ids.push(fact_uri.to_string());
                        continue;
                    }

                    if let Some(pattern) = target.get("pattern") {
                        let entity = pattern
                            .get("entity")
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow!("pattern.entity is required"))?;
                        let field = pattern
                            .get("field")
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow!("pattern.field is required"))?;
                        let value = pattern
                            .get("value")
                            .ok_or_else(|| anyhow!("pattern.value is required"))?;
                        let parsed_value = parse_rfd_value(value)?;
                        let entity_uri = Uri::parse(entity)?;
                        let field_uri = Uri::parse(field)?;
                        let candidates = memory
                            .list_facts(Some(&entity_uri), Some(&field_uri), false, 5000)
                            .await?;
                        for candidate in candidates {
                            if candidate.value == parsed_value {
                                retract_ids.push(candidate.fact_id.to_string());
                            }
                        }
                    }
                }

                if retract_ids.is_empty() {
                    let out = json!({
                        "txId": Uri::parse(format!("borg:tx:{}", uuid::Uuid::now_v7()))?.to_string(),
                        "retractionFactUris": Vec::<String>::new(),
                    });
                    return Ok(ToolResponse {
                        content: ToolResultData::Text(serde_json::to_string(&out)?),
                    });
                }

                let mut originals = Vec::new();
                for fact_id in &retract_ids {
                    if let Some(fact) = memory.get_fact(fact_id).await? {
                        originals.push(fact);
                    }
                }
                let original_ids: Vec<String> =
                    originals.iter().map(|fact| fact.fact_id.to_string()).collect();
                let _ = memory.mark_facts_retracted(&original_ids).await?;

                let retraction_inputs: Vec<FactInput> = originals
                    .into_iter()
                    .map(|fact| FactInput {
                        source: source_uri.clone(),
                        entity: fact.entity,
                        field: fact.field,
                        arity: fact.arity,
                        value: fact.value,
                    })
                    .collect();
                let result = memory.state_facts(retraction_inputs).await?;
                let retraction_ids: Vec<String> = result
                    .facts
                    .iter()
                    .map(|fact| fact.fact_id.to_string())
                    .collect();
                let _ = memory.mark_facts_retracted(&retraction_ids).await?;
                let out = json!({
                    "txId": result.tx_id.to_string(),
                    "retractionFactUris": retraction_ids
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(list_facts_spec, None, move |request| {
            let memory = memory_for_list.clone();
            async move {
                let entity = request
                    .arguments
                    .get("entity")
                    .and_then(Value::as_str)
                    .map(Uri::parse)
                    .transpose()?;
                let field = request
                    .arguments
                    .get("field")
                    .and_then(Value::as_str)
                    .map(Uri::parse)
                    .transpose()?;
                let include_retracted = request
                    .arguments
                    .get("includeRetracted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let limit = request
                    .arguments
                    .get("pagination")
                    .and_then(|value| value.get("limit"))
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(50);
                let facts = memory
                    .list_facts(
                        entity.as_ref(),
                        field.as_ref(),
                        include_retracted,
                        limit.max(1),
                    )
                    .await?;
                let out = json!({
                    "facts": facts.iter().map(|fact| {
                        json!({
                            "factUri": fact.fact_id.to_string(),
                            "source": fact.source.to_string(),
                            "entity": fact.entity.to_string(),
                            "field": fact.field.to_string(),
                            "value": fact_value_to_tool_json(&fact.value),
                            "txId": fact.tx_id.to_string(),
                            "statedAt": fact.stated_at.to_rfc3339(),
                        })
                    }).collect::<Vec<_>>(),
                    "warnings": Vec::<Value>::new()
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(
            define_namespace_spec,
            None,
            move |request| {
                let memory = memory_for_define_namespace.clone();
                async move {
                    let namespace_uri = request
                        .arguments
                        .get("namespaceUri")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            anyhow!("Memory-Schema-defineNamespace requires namespaceUri")
                        })?;
                    let prefix = request
                        .arguments
                        .get("prefix")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("Memory-Schema-defineNamespace requires prefix"))?;
                    let source = request
                        .arguments
                        .get("source")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow!("Memory-Schema-defineNamespace requires source"))?;
                    let label = request
                        .arguments
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(prefix);
                    let description = request
                        .arguments
                        .get("description")
                        .and_then(Value::as_str);
                    let namespace_uri = Uri::parse(namespace_uri)?;
                    let source_uri = Uri::parse(source)?;

                    let mut facts = vec![
                        FactInput {
                            source: source_uri.clone(),
                            entity: namespace_uri.clone(),
                            field: Uri::parse("borg:field:type")?,
                            arity: Default::default(),
                            value: FactValue::Ref(Uri::parse("borg:kind:namespace")?),
                        },
                        FactInput {
                            source: source_uri.clone(),
                            entity: namespace_uri.clone(),
                            field: Uri::parse("borg:field:label")?,
                            arity: Default::default(),
                            value: FactValue::Text(label.to_string()),
                        },
                        FactInput {
                            source: source_uri.clone(),
                            entity: namespace_uri.clone(),
                            field: Uri::parse("borg:field:alias")?,
                            arity: Default::default(),
                            value: FactValue::Text(prefix.to_string()),
                        },
                    ];
                    if let Some(description) = description {
                        facts.push(FactInput {
                            source: source_uri,
                            entity: namespace_uri.clone(),
                            field: Uri::parse("borg:field:description")?,
                            arity: Default::default(),
                            value: FactValue::Text(description.to_string()),
                        });
                    }
                    let result = memory.state_facts(facts).await?;
                    let out = json!({
                        "namespaceUri": namespace_uri.to_string(),
                        "txId": result.tx_id.to_string(),
                        "facts": result.facts.iter().map(|fact| fact.fact_id.to_string()).collect::<Vec<_>>()
                    });
                    Ok(ToolResponse {
                        content: ToolResultData::Text(serde_json::to_string(&out)?),
                    })
                }
            },
        ))?
        .add_tool(Tool::new(define_kind_spec, None, move |request| {
            let memory = memory_for_define_kind.clone();
            async move {
                let kind_uri = request
                    .arguments
                    .get("kindUri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineKind requires kindUri"))?;
                let source = request
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineKind requires source"))?;
                let label = request
                    .arguments
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(kind_uri);
                let description = request
                    .arguments
                    .get("description")
                    .and_then(Value::as_str);
                let kind_uri = Uri::parse(kind_uri)?;
                let source_uri = Uri::parse(source)?;
                let mut facts = vec![
                    FactInput {
                        source: source_uri.clone(),
                        entity: kind_uri.clone(),
                        field: Uri::parse("borg:field:type")?,
                        arity: Default::default(),
                        value: FactValue::Ref(Uri::parse("borg:kind:kind")?),
                    },
                    FactInput {
                        source: source_uri.clone(),
                        entity: kind_uri.clone(),
                        field: Uri::parse("borg:field:label")?,
                        arity: Default::default(),
                        value: FactValue::Text(label.to_string()),
                    },
                ];
                if let Some(description) = description {
                    facts.push(FactInput {
                        source: source_uri,
                        entity: kind_uri.clone(),
                        field: Uri::parse("borg:field:description")?,
                        arity: Default::default(),
                        value: FactValue::Text(description.to_string()),
                    });
                }
                let result = memory.state_facts(facts).await?;
                let out = json!({
                    "kindUri": kind_uri.to_string(),
                    "txId": result.tx_id.to_string(),
                    "facts": result.facts.iter().map(|fact| fact.fact_id.to_string()).collect::<Vec<_>>()
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(define_field_spec, None, move |request| {
            let memory = memory_for_define_field.clone();
            async move {
                let field_uri = request
                    .arguments
                    .get("fieldUri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineField requires fieldUri"))?;
                let domain = request
                    .arguments
                    .get("domain")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineField requires domain"))?;
                let range = request
                    .arguments
                    .get("range")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineField requires range"))?;
                let allows_many = request
                    .arguments
                    .get("allowsMany")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineField requires allowsMany"))?;
                let source = request
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Memory-Schema-defineField requires source"))?;
                let label = request
                    .arguments
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(field_uri);
                let description = request
                    .arguments
                    .get("description")
                    .and_then(Value::as_str);
                let field_uri = Uri::parse(field_uri)?;
                let source_uri = Uri::parse(source)?;

                let mut facts = vec![
                    FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:type")?,
                        arity: Default::default(),
                        value: FactValue::Ref(Uri::parse("borg:kind:field")?),
                    },
                    FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:label")?,
                        arity: Default::default(),
                        value: FactValue::Text(label.to_string()),
                    },
                    FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:allowsMany")?,
                        arity: Default::default(),
                        value: FactValue::Boolean(allows_many),
                    },
                ];
                if let Some(description) = description {
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:description")?,
                        arity: Default::default(),
                        value: FactValue::Text(description.to_string()),
                    });
                }
                for domain_kind in domain {
                    let domain_kind = domain_kind
                        .as_str()
                        .ok_or_else(|| anyhow!("domain values must be strings"))?;
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:domainKind")?,
                        arity: crate::FactArity::Many,
                        value: FactValue::Ref(Uri::parse(domain_kind)?),
                    });
                }
                for range_kind in range {
                    let range_kind = range_kind
                        .as_str()
                        .ok_or_else(|| anyhow!("range values must be strings"))?;
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:rangeKind")?,
                        arity: crate::FactArity::Many,
                        value: FactValue::Ref(Uri::parse(range_kind)?),
                    });
                }
                if let Some(value) = request.arguments.get("isTransitive").and_then(Value::as_bool) {
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:isTransitive")?,
                        arity: Default::default(),
                        value: FactValue::Boolean(value),
                    });
                }
                if let Some(value) = request.arguments.get("isReflexive").and_then(Value::as_bool) {
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:isReflexive")?,
                        arity: Default::default(),
                        value: FactValue::Boolean(value),
                    });
                }
                if let Some(value) = request.arguments.get("isSymmetric").and_then(Value::as_bool) {
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:isSymmetric")?,
                        arity: Default::default(),
                        value: FactValue::Boolean(value),
                    });
                }
                if let Some(value) = request.arguments.get("inverseOf").and_then(Value::as_str) {
                    facts.push(FactInput {
                        source: source_uri.clone(),
                        entity: field_uri.clone(),
                        field: Uri::parse("borg:field:inverseOf")?,
                        arity: Default::default(),
                        value: FactValue::Ref(Uri::parse(value)?),
                    });
                }

                let result = memory.state_facts(facts).await?;
                let out = json!({
                    "fieldUri": field_uri.to_string(),
                    "txId": result.tx_id.to_string(),
                    "facts": result.facts.iter().map(|fact| fact.fact_id.to_string()).collect::<Vec<_>>()
                });
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&out)?),
                })
            }
        }))?
        .add_tool(Tool::new(get_schema_spec, None, move |_request| async move {
            Ok(ToolResponse {
                content: ToolResultData::Text(memory_get_schema_context().to_string()),
            })
        }))?
        .add_tool(Tool::new(
            new_entity_spec,
            None,
            move |request| {
                let memory = memory_for_new_entity.clone();
                async move {
                let ns = request
                    .arguments
                    .get("ns")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow!("Memory-newEntity tool requires ns"))?;
                let kind = request
                    .arguments
                    .get("kind")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow!("Memory-newEntity tool requires kind"))?;
                let display_name = request
                    .arguments
                    .get("displayName")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow!("Memory-newEntity tool requires non-empty displayName"))?;
                let entity = Uri::from_parts(ns, kind, Some(&uuid::Uuid::now_v7().to_string()))?;
                let source = Uri::parse(format!("borg:tool_call:{}", uuid::Uuid::now_v7()))?;
                let field = Uri::parse("borg:field:displayName")?;
                memory
                    .state_facts(vec![FactInput {
                        source,
                        entity: entity.clone(),
                        field,
                        arity: Default::default(),
                        value: FactValue::Text(display_name.to_string()),
                    }])
                    .await?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(entity.to_string()),
                })
                }
            },
        ))?
        .add_tool(Tool::new(save_facts_spec, None, move |request| {
            let memory = memory_for_save.clone();
            async move {
                let entities_value = request
                    .arguments
                    .get("entities")
                    .cloned()
                    .ok_or_else(|| anyhow!("Memory-saveFacts tool requires entities"))?;
                let entities: Vec<String> = serde_json::from_value(entities_value)?;
                if entities.is_empty() {
                    return Err(anyhow!(
                        "Memory-saveFacts expects a non-empty entities array"
                    ));
                }
                let declared_entities: HashSet<String> = entities.into_iter().collect();
                let facts_value = request
                    .arguments
                    .get("facts")
                    .cloned()
                    .ok_or_else(|| anyhow!("Memory-saveFacts tool requires facts"))?;
                let facts: Vec<FactInput> = serde_json::from_value(facts_value)?;
                if facts.is_empty() {
                    return Err(anyhow!("Memory-saveFacts expects a non-empty facts array"));
                }
                if let Some(missing) = facts
                    .iter()
                    .map(|fact| fact.entity.to_string())
                    .find(|entity| !declared_entities.contains(entity))
                {
                    return Err(anyhow!(
                        "Memory-saveFacts facts contain undeclared entity `{}`; include it in `entities`",
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
                    .ok_or_else(|| anyhow!("Memory-searchMemory tool requires query"))?;
                let query: SearchQuery = serde_json::from_value(query_value)?;
                let results = memory.search_query(query).await?;
                Ok(ToolResponse {
                    content: ToolResultData::Text(serde_json::to_string(&results)?),
                })
            }
        }))?
        .build()
}

fn parse_rfd_value(value: &Value) -> Result<FactValue> {
    if let Some(text) = value.get("string").and_then(Value::as_str) {
        return Ok(FactValue::Text(text.to_string()));
    }
    if let Some(number) = value.get("number").and_then(Value::as_f64) {
        return Ok(FactValue::Float(number));
    }
    if let Some(boolean) = value.get("bool").and_then(Value::as_bool) {
        return Ok(FactValue::Boolean(boolean));
    }
    if let Some(date) = value.get("date").and_then(Value::as_str) {
        return Ok(FactValue::Text(date.to_string()));
    }
    if let Some(datetime) = value.get("datetime").and_then(Value::as_str) {
        return Ok(FactValue::Text(datetime.to_string()));
    }
    if let Some(uri) = value.get("uri").and_then(Value::as_str) {
        return Ok(FactValue::Ref(Uri::parse(uri)?));
    }
    if let Some(text) = value.get("Text").and_then(Value::as_str) {
        return Ok(FactValue::Text(text.to_string()));
    }
    if let Some(integer) = value.get("Integer").and_then(Value::as_i64) {
        return Ok(FactValue::Integer(integer));
    }
    if let Some(number) = value.get("Float").and_then(Value::as_f64) {
        return Ok(FactValue::Float(number));
    }
    if let Some(boolean) = value.get("Boolean").and_then(Value::as_bool) {
        return Ok(FactValue::Boolean(boolean));
    }
    if let Some(bytes) = value.get("Bytes").and_then(Value::as_array) {
        let mut parsed = Vec::with_capacity(bytes.len());
        for b in bytes {
            let raw = b
                .as_u64()
                .ok_or_else(|| anyhow!("Bytes values must be unsigned integers"))?;
            if raw > u8::MAX as u64 {
                return Err(anyhow!("Bytes values must be <= 255"));
            }
            parsed.push(raw as u8);
        }
        return Ok(FactValue::Bytes(parsed));
    }
    if let Some(uri) = value.get("Ref").and_then(Value::as_str) {
        return Ok(FactValue::Ref(Uri::parse(uri)?));
    }
    Err(anyhow!(
        "unsupported fact value format; expected one of string/number/bool/date/datetime/uri"
    ))
}

fn fact_value_to_tool_json(value: &FactValue) -> Value {
    match value {
        FactValue::Text(text) => json!({ "string": text }),
        FactValue::Integer(v) => json!({ "number": *v as f64 }),
        FactValue::Float(v) => json!({ "number": v }),
        FactValue::Boolean(v) => json!({ "bool": v }),
        FactValue::Bytes(v) => json!({ "bytes": v }),
        FactValue::Ref(uri) => json!({ "uri": uri.to_string() }),
    }
}

fn required_default_tool_spec(name: &str) -> Result<ToolSpec> {
    default_tool_specs()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| anyhow!("missing default tool spec `{}`", name))
}

fn memory_get_schema_context() -> &'static str {
    r#"you are borg's memory writer. your job is to convert user messages into high-quality rdf-style facts using the tools provided.

you write facts in the form:
  { source, entity, field, value }

where:
- source is the uri/curie of the most specific provenance (prefer message uri, else session uri).
- entity is the subject uri/curie.
- field is the predicate uri/curie (must be from the registry or created via the proposal process).
- value is a typed object: { type, data } where type in { string, number, boolean, datetime, uri, entityRef, json }.

critical goals
1) build an entity graph (separate entities + relationships), not a pile of user-centric "compound fields".
2) reuse canonical fields and kinds; do not invent near-duplicates.
3) if something truly needs a new field/kind, propose it explicitly as first-class entities (borg:kind:field / borg:kind:kind) and map it to a canonical field if possible.
4) every fact must be attributable to a source.

tools you have
- searchMemory(query: string) -> returns existing entities/fields/kinds (fuzzy).
- newEntity(namespace: string, kind: string, label?: string) -> creates a new entity uri/curie.
- stateFacts(facts: fact[]) -> writes facts.

runtime placeholders (provided by the host app)
- current source uri/curie: {{currentSource}}
- user entity uri/curie: {{userEntity}} (the user/person entity representing the current user)

prefixes (curie style)
- borg: borg:
- rdf: rdf:
- rdfs: rdfs:
- schema: schema:
- prov: prov:
- xsd: xsd:
- imdb: imdb:

baseline kinds (use rdf:type to assign)
- borg:kind:entity
- borg:kind:kind            (kinds/classes are entities too)
- borg:kind:field           (fields/properties are entities too)
- borg:kind:namespace
- borg:kind:valueType
- borg:kind:cardinality
- borg:kind:person
- borg:kind:relationship    (reified relationship node)
- borg:kind:file
- borg:kind:folder

baseline canonical fields (preferred vocabulary)
- rdf:type                  (entity -> kind uri)
- rdfs:label                (human label)
- rdfs:comment              (description)
- schema:name               (canonical name)
- schema:alternateName      (nicknames/aliases)
- schema:email

schema-definition fields (used only when defining kinds/fields)
- borg:field:inNamespace
- borg:field:valueType
- borg:field:cardinality
- borg:field:domainKind
- borg:field:rangeKind
- borg:field:canonicalField
- borg:field:alias
- borg:field:deprecated
- borg:field:extendsKind
- borg:field:recommendedField
- borg:field:identityField

relationship reification fields (used on borg:kind:relationship entities)
- borg:field:subject            (entityRef)
- borg:field:predicate          (uri of a field)
- borg:field:object             (entityRef)
- borg:field:relationshipLabel  (string, e.g., "girlfriend")
- borg:field:asOf               (datetime)

file/folder fields
- borg:field:path
- borg:field:mimeType
- borg:field:parentFolder
- borg:field:contains

hard rules (do not violate)
- do not create random fields like "girlfriendName", "userGirlfriend", "movieDownloadFolderName", etc.
  instead: create distinct entities and connect them via relationship entities or existing canonical fields.
- do not create a new field or kind until you have searched for an existing one that fits.
- do not write facts for information that is clearly transient (jokes, one-off prompts, speculative).
- if the user says something uncertain ("maybe", "i guess", "might"), either skip memory or mark it as such only if you have a supported pattern (otherwise skip).
- keep ids/labels lowercase or camelCase. avoid spaces in ids.

memory writing procedure (always follow)
step 0: decide if there is any durable memory.
- durable examples: names, preferences, stable relationships, recurring folders, provider settings, canonical identifiers.
- not durable: temporary tasks, ephemeral chat content, one-time requests unless explicitly stated as a preference/rule.

step 1: resolve entities that already exist.
- call searchMemory with key strings (names, emails, folder paths, known ids).
- if an entity already exists, reuse its uri.
- if multiple candidates exist, prefer the one already linked to {{userEntity}} or with matching schema:name / rdfs:label.

step 2: create missing entities with newEntity.
for every new entity you create, immediately record:
- rdf:type
- rdfs:label
and for persons if known:
- schema:name (canonical)
- schema:alternateName (nicknames) as needed

step 3: write facts using canonical fields.
- prefer schema:name for canonical names.
- prefer schema:alternateName for nicknames/aliases.
- use relationship reification for roles like girlfriend/coworker/manager unless you already have a canonical relation field.

relationship modeling rule (important)
when you need to represent "a is related to b as <role>":
- create a new relationship entity (borg namespace, kind relationship).
- set:
  relationship rdf:type borg:kind:relationship
  relationship borg:field:subject -> a (entityRef)
  relationship borg:field:object -> b (entityRef)
  relationship borg:field:relationshipLabel -> "<role>" (string, lowercase)
  relationship borg:field:predicate -> a canonical relation field uri (see next rule)

predicate selection rule
- first searchMemory for a suitable canonical relation field (e.g., borg:field:relatedTo, schema:relatedTo, etc.).
- if you find one, reuse it.
- if you do not find one, propose ONE general-purpose canonical relation field (not role-specific):
  - preferred id: borg:field:relatedTo
  - valueType: entityRef
  - cardinality: many
  - domainKind: borg:kind:entity
  - rangeKind: borg:kind:entity
  - aliases: ["relation", "relationship", "linkedTo"]
  then use it as the predicate for relationship entities.
note: do NOT create role-specific predicates like borg:field:girlfriendOf unless there is a strong product reason.

field/kind proposal process (only if truly needed)
if you must introduce a new field:
1) searchMemory for existing field candidates (by label + synonyms).
2) if none fits, create a field entity:
   newEntity(namespace="borg", kind="field", label="<camelCaseFieldName>")
3) state its definition facts:
   - rdf:type -> borg:kind:field
   - rdfs:label -> "<label>"
   - rdfs:comment -> "<short description>"
   - borg:field:inNamespace -> borg:namespace:borg (or the correct namespace entity)
   - borg:field:valueType -> one of borg:valueType:*
   - borg:field:cardinality -> borg:cardinality:one|many
   - borg:field:domainKind -> applicable kinds
   - borg:field:rangeKind -> applicable kinds (if entityRef)
   - borg:field:alias -> list of lowercase synonyms
4) if it duplicates an existing concept, set borg:field:canonicalField to the canonical one and mark deprecated=true.

same idea for new kinds:
- create a kind entity (kind="kind"), rdf:type borg:kind:kind, set label/comment, extendsKind, recommendedField, identityField.

fact formatting rules
- always set source to {{currentSource}} unless the host provides a more specific message uri.
- value examples:
  string:    { "type": "string", "data": "mariana" }
  entityRef: { "type": "entityRef", "data": "borg:person:abc123" }
  uri:       { "type": "uri", "data": "borg:kind:person" }
  datetime:  { "type": "datetime", "data": "2026-02-27T18:12:00-06:00" }

write discipline
- prefer one stateFacts call per message (batch facts).
- only write what is supported by the user's statement.
- avoid duplication: if a fact already exists, do not restate unless you are correcting it.

quick example (mental model only; do not include unless asked)
user: "my girlfriend's name is mariana, but she goes by maya."
=> create/reuse: {{userEntity}} (person), girlfriend entity (person), relationship entity
=> facts:
- girlfriend schema:name "mariana"
- girlfriend schema:alternateName "maya"
- relationship subject {{userEntity}}, object girlfriend, relationshipLabel "girlfriend", predicate borg:field:relatedTo (or existing equivalent)

your output must be tool calls only when writing memory.
if no durable memory is present, do not call tools.
"#
}
