use anyhow::Result;
use borg_agent::ToolRunner;
use clap::{Args, Subcommand};
use serde_json::{Map, Value, json};

use crate::app::BorgCliApp;

#[derive(Subcommand, Debug)]
pub enum MemoryToolsCommand {
    #[command(about = "List Memory commands")]
    List,
    #[command(about = "Write a batch of memory facts")]
    StateFacts(StateFactsArgs),
    #[command(about = "Fuzzy-search memory entities and schema")]
    Search(SearchArgs),
    #[command(about = "Create an entity URI for a kind")]
    CreateEntity(CreateEntityArgs),
    #[command(about = "Get one entity by URI")]
    GetEntity(GetEntityArgs),
    #[command(about = "Retract facts by URI or exact pattern")]
    RetractFacts(PayloadArgs),
    #[command(about = "List facts with filters")]
    ListFacts(ListFactsArgs),
    #[command(about = "Get baseline memory schema")]
    GetSchema(PayloadArgs),
    #[command(about = "Create a new memory entity")]
    NewEntity(PayloadArgs),
    #[command(about = "Save memory facts to an entity")]
    SaveFacts(PayloadArgs),
    #[command(about = "Search memory entities by query")]
    SearchMemory(SearchMemoryArgs),
    #[command(about = "Schema definition commands")]
    Schema {
        #[command(subcommand)]
        cmd: MemorySchemaCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum MemorySchemaCommand {
    #[command(about = "Define a namespace schema entity")]
    DefineNamespace(DefineNamespaceArgs),
    #[command(about = "Define a kind schema entity")]
    DefineKind(DefineKindArgs),
    #[command(about = "Define a field schema entity")]
    DefineField(DefineFieldArgs),
}

#[derive(Args, Debug)]
pub struct RawPayloadArg {
    #[arg(long, value_name = "JSON", help = "Raw JSON payload override")]
    pub payload_json: Option<String>,
}

#[derive(Args, Debug)]
pub struct PayloadArgs {
    #[arg(
        value_name = "PAYLOAD_JSON",
        default_value = "{}",
        help = "JSON payload for this tool"
    )]
    pub payload: String,
}

#[derive(Args, Debug)]
pub struct StateFactsArgs {
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub stated_at: Option<String>,
    #[arg(long, value_name = "JSON_ARRAY", help = "Facts array JSON")]
    pub facts_json: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long = "result-type")]
    pub result_types: Vec<String>,
    #[arg(long = "namespace-prefix")]
    pub namespace_prefixes: Vec<String>,
    #[arg(long = "kind-uri")]
    pub kind_uris: Vec<String>,
    #[arg(long = "field-uri")]
    pub field_uris: Vec<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct CreateEntityArgs {
    #[arg(long)]
    pub kind_uri: Option<String>,
    #[arg(long)]
    pub entity_uri: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct GetEntityArgs {
    #[arg(long)]
    pub entity_uri: Option<String>,
    #[arg(long)]
    pub include_retracted: Option<bool>,
    #[arg(long)]
    pub fact_limit: Option<u64>,
    #[arg(long)]
    pub fact_cursor: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct ListFactsArgs {
    #[arg(long)]
    pub entity: Option<String>,
    #[arg(long)]
    pub field: Option<String>,
    #[arg(long)]
    pub include_retracted: Option<bool>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct SearchMemoryArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub ns: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct DefineNamespaceArgs {
    #[arg(long)]
    pub namespace_uri: Option<String>,
    #[arg(long)]
    pub prefix: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct DefineKindArgs {
    #[arg(long)]
    pub kind_uri: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

#[derive(Args, Debug)]
pub struct DefineFieldArgs {
    #[arg(long)]
    pub field_uri: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "domain-uri")]
    pub domain: Vec<String>,
    #[arg(long = "range-uri")]
    pub range: Vec<String>,
    #[arg(long)]
    pub allows_many: Option<bool>,
    #[arg(long)]
    pub is_transitive: Option<bool>,
    #[arg(long)]
    pub is_reflexive: Option<bool>,
    #[arg(long)]
    pub is_symmetric: Option<bool>,
    #[arg(long)]
    pub inverse_of: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[command(flatten)]
    pub raw: RawPayloadArg,
}

pub fn command_names() -> Vec<&'static str> {
    vec![
        "state-facts",
        "search",
        "create-entity",
        "get-entity",
        "retract-facts",
        "list-facts",
        "schema define-namespace",
        "schema define-kind",
        "schema define-field",
        "get-schema",
        "new-entity",
        "save-facts",
        "search-memory",
    ]
}

pub async fn run(app: &BorgCliApp, cmd: MemoryToolsCommand) -> Result<Value> {
    match cmd {
        MemoryToolsCommand::List => {
            Ok(json!({"ok": true, "namespace": "memory", "commands": command_names()}))
        }
        MemoryToolsCommand::StateFacts(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "source", args.source);
            insert_opt(&mut map, "statedAt", args.stated_at);
            if let Some(facts) = args.facts_json {
                map.insert("facts".to_string(), serde_json::from_str(&facts)?);
            }
            execute(
                app,
                "state-facts",
                "Memory-stateFacts",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::Search(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "query", args.query);
            insert_vec(&mut map, "resultTypes", args.result_types);
            insert_vec(&mut map, "namespacePrefixes", args.namespace_prefixes);
            insert_vec(&mut map, "kindUris", args.kind_uris);
            insert_vec(&mut map, "fieldUris", args.field_uris);
            let mut pagination = Map::new();
            if let Some(limit) = args.limit {
                pagination.insert("limit".to_string(), Value::from(limit));
            }
            if let Some(cursor) = args.cursor {
                pagination.insert("cursor".to_string(), Value::String(cursor));
            }
            if !pagination.is_empty() {
                map.insert("pagination".to_string(), Value::Object(pagination));
            }
            execute(
                app,
                "search",
                "Memory-search",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::CreateEntity(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "kindUri", args.kind_uri);
            insert_opt(&mut map, "entityUri", args.entity_uri);
            insert_opt(&mut map, "label", args.label);
            insert_opt(&mut map, "description", args.description);
            insert_opt(&mut map, "source", args.source);
            execute(
                app,
                "create-entity",
                "Memory-createEntity",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::GetEntity(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "entityUri", args.entity_uri);
            if let Some(include) = args.include_retracted {
                map.insert("includeRetracted".to_string(), Value::Bool(include));
            }
            let mut fact_pagination = Map::new();
            if let Some(limit) = args.fact_limit {
                fact_pagination.insert("limit".to_string(), Value::from(limit));
            }
            if let Some(cursor) = args.fact_cursor {
                fact_pagination.insert("cursor".to_string(), Value::String(cursor));
            }
            if !fact_pagination.is_empty() {
                map.insert("factPagination".to_string(), Value::Object(fact_pagination));
            }
            execute(
                app,
                "get-entity",
                "Memory-getEntity",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::RetractFacts(args) => {
            execute(app, "retract-facts", "Memory-retractFacts", args.payload).await
        }
        MemoryToolsCommand::ListFacts(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "entity", args.entity);
            insert_opt(&mut map, "field", args.field);
            if let Some(include) = args.include_retracted {
                map.insert("includeRetracted".to_string(), Value::Bool(include));
            }
            insert_opt(&mut map, "since", args.since);
            insert_opt(&mut map, "until", args.until);
            let mut pagination = Map::new();
            if let Some(limit) = args.limit {
                pagination.insert("limit".to_string(), Value::from(limit));
            }
            if let Some(cursor) = args.cursor {
                pagination.insert("cursor".to_string(), Value::String(cursor));
            }
            if !pagination.is_empty() {
                map.insert("pagination".to_string(), Value::Object(pagination));
            }
            execute(
                app,
                "list-facts",
                "Memory-listFacts",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::GetSchema(args) => {
            execute(app, "get-schema", "Memory-getSchema", args.payload).await
        }
        MemoryToolsCommand::NewEntity(args) => {
            execute(app, "new-entity", "Memory-newEntity", args.payload).await
        }
        MemoryToolsCommand::SaveFacts(args) => {
            execute(app, "save-facts", "Memory-saveFacts", args.payload).await
        }
        MemoryToolsCommand::SearchMemory(args) => {
            let mut map = Map::new();
            insert_opt(&mut map, "query", args.query);
            insert_opt(&mut map, "ns", args.ns);
            insert_opt(&mut map, "kind", args.kind);
            if let Some(limit) = args.limit {
                map.insert("limit".to_string(), Value::from(limit));
            }
            execute(
                app,
                "search-memory",
                "Memory-searchMemory",
                payload(args.raw.payload_json, map)?,
            )
            .await
        }
        MemoryToolsCommand::Schema { cmd } => match cmd {
            MemorySchemaCommand::DefineNamespace(args) => {
                let mut map = Map::new();
                insert_opt(&mut map, "namespaceUri", args.namespace_uri);
                insert_opt(&mut map, "prefix", args.prefix);
                insert_opt(&mut map, "label", args.label);
                insert_opt(&mut map, "description", args.description);
                insert_opt(&mut map, "source", args.source);
                execute(
                    app,
                    "schema define-namespace",
                    "Memory-Schema-defineNamespace",
                    payload(args.raw.payload_json, map)?,
                )
                .await
            }
            MemorySchemaCommand::DefineKind(args) => {
                let mut map = Map::new();
                insert_opt(&mut map, "kindUri", args.kind_uri);
                insert_opt(&mut map, "label", args.label);
                insert_opt(&mut map, "description", args.description);
                insert_opt(&mut map, "source", args.source);
                execute(
                    app,
                    "schema define-kind",
                    "Memory-Schema-defineKind",
                    payload(args.raw.payload_json, map)?,
                )
                .await
            }
            MemorySchemaCommand::DefineField(args) => {
                let mut map = Map::new();
                insert_opt(&mut map, "fieldUri", args.field_uri);
                insert_opt(&mut map, "label", args.label);
                insert_opt(&mut map, "description", args.description);
                insert_vec(&mut map, "domain", args.domain);
                insert_vec(&mut map, "range", args.range);
                insert_bool_opt(&mut map, "allowsMany", args.allows_many);
                insert_bool_opt(&mut map, "isTransitive", args.is_transitive);
                insert_bool_opt(&mut map, "isReflexive", args.is_reflexive);
                insert_bool_opt(&mut map, "isSymmetric", args.is_symmetric);
                insert_opt(&mut map, "inverseOf", args.inverse_of);
                insert_opt(&mut map, "source", args.source);
                execute(
                    app,
                    "schema define-field",
                    "Memory-Schema-defineField",
                    payload(args.raw.payload_json, map)?,
                )
                .await
            }
        },
    }
}

fn insert_opt(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_vec(map: &mut Map<String, Value>, key: &str, values: Vec<String>) {
    if !values.is_empty() {
        map.insert(
            key.to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        );
    }
}

fn insert_bool_opt(map: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::Bool(value));
    }
}

fn payload(raw: Option<String>, map: Map<String, Value>) -> Result<String> {
    if let Some(raw) = raw {
        return Ok(raw);
    }
    Ok(Value::Object(map).to_string())
}

async fn execute(
    app: &BorgCliApp,
    command: &str,
    tool_name: &str,
    payload: String,
) -> Result<Value> {
    let arguments: Value = serde_json::from_str(&payload)
        .map_err(|err| anyhow::anyhow!("invalid JSON payload: {} (payload={})", err, payload))?;
    let memory = app.open_memory_store().await?;
    let toolchain = borg_memory::build_memory_toolchain(memory)?;
    let response = toolchain
        .run(borg_agent::ToolRequest {
            tool_call_id: format!("cli-memory-{}", command.replace(' ', "-")),
            tool_name: tool_name.to_string(),
            arguments,
        })
        .await?;

    Ok(json!({
        "ok": true,
        "namespace": "memory",
        "command": command,
        "tool": tool_name,
        "content": response.content
    }))
}
