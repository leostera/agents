use std::path::PathBuf;

use borg_agent::{ToolRequest, ToolResultData};
use borg_memory::{
    FactArity, FactInput, FactValue, MemoryStore, Uri, build_memory_toolchain,
    default_memory_tool_specs,
};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
struct RfdCase {
    id: &'static str,
    area: &'static str,
    summary: &'static str,
}

fn rfd0005_cases() -> Vec<RfdCase> {
    vec![
        RfdCase {
            id: "tools.namespacing",
            area: "tools",
            summary: "Memory-* MCP tool names and canonical surface exist",
        },
        RfdCase {
            id: "tools.state_facts_atomic",
            area: "tools",
            summary: "Memory-stateFacts behaves as single-call transaction boundary",
        },
        RfdCase {
            id: "tools.retract_by_fact_uri",
            area: "tools",
            summary: "Memory-retractFacts retracts exact fact by factUri",
        },
        RfdCase {
            id: "tools.retract_by_pattern",
            area: "tools",
            summary: "Memory-retractFacts retracts exact (entity, field, value) matches",
        },
        RfdCase {
            id: "types.uri_format",
            area: "types",
            summary: "Uri accepts RFC3986 values used by memory records",
        },
        RfdCase {
            id: "types.value_schema",
            area: "types",
            summary: "Fact value encoding matches RFD typed value contract",
        },
        RfdCase {
            id: "schema.define_namespace",
            area: "schema",
            summary: "Memory-Schema-defineNamespace writes schema facts",
        },
        RfdCase {
            id: "schema.define_kind",
            area: "schema",
            summary: "Memory-Schema-defineKind writes schema facts",
        },
        RfdCase {
            id: "schema.define_field_core",
            area: "schema",
            summary: "Memory-Schema-defineField persists domain/range/allowsMany metadata",
        },
        RfdCase {
            id: "schema.define_field_relations",
            area: "schema",
            summary: "Memory-Schema-defineField supports relation semantics fields",
        },
        RfdCase {
            id: "schema.bootstrap",
            area: "schema",
            summary: "Bootstrap facts can be written before full schema is present",
        },
        RfdCase {
            id: "identity.same_as_closure",
            area: "identity",
            summary: "sameAs is resolved symmetrically/transitively on reads",
        },
        RfdCase {
            id: "identity.canonical_preference",
            area: "identity",
            summary: "borg:* identity is preferred as canonical when equivalent",
        },
        RfdCase {
            id: "read.warnings_shape",
            area: "read",
            summary: "Warning payloads are structured and machine-actionable",
        },
        RfdCase {
            id: "read.warnings_domain_mismatch",
            area: "read",
            summary: "Domain mismatch warnings are emitted for violating facts",
        },
        RfdCase {
            id: "read.warnings_range_mismatch",
            area: "read",
            summary: "Range mismatch warnings are emitted for violating facts",
        },
        RfdCase {
            id: "read.warnings_cardinality",
            area: "read",
            summary: "Cardinality warnings are emitted for violating facts",
        },
        RfdCase {
            id: "read.warnings_unknown_value_type",
            area: "read",
            summary: "Unknown value type warnings are emitted when decoding fails",
        },
        RfdCase {
            id: "observability.tx_id_all_writes",
            area: "observability",
            summary: "All write operations return server-generated txId",
        },
        RfdCase {
            id: "observability.tx_id_non_reusable",
            area: "observability",
            summary: "txIds are not client-supplied and do not span multiple calls",
        },
    ]
}

#[test]
fn rfd0005_suite_matrix_is_defined_and_unique() {
    let cases = rfd0005_cases();
    assert!(!cases.is_empty(), "RFD0005 test matrix must not be empty");

    let mut ids: Vec<&str> = cases.iter().map(|case| case.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), cases.len(), "RFD0005 test IDs must be unique");

    let covered_areas: std::collections::BTreeSet<&str> =
        cases.iter().map(|case| case.area).collect();
    assert!(
        covered_areas.contains("tools")
            && covered_areas.contains("schema")
            && covered_areas.contains("identity")
            && covered_areas.contains("read")
            && covered_areas.contains("observability"),
        "RFD0005 matrix must cover all primary contract areas"
    );
    assert!(
        cases.iter().all(|case| !case.summary.trim().is_empty()),
        "RFD0005 cases must include non-empty summaries"
    );
}

#[test]
fn rfd0005_current_tools_expose_schema_anchor() {
    let names: Vec<String> = default_memory_tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    assert!(
        names.iter().any(|name| name == "Memory-getSchema"),
        "Memory-getSchema must remain available while RFD0005 tools are implemented"
    );
}

#[test]
fn rfd0005_tools_surface_contract() {
    let names: Vec<String> = default_memory_tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    let expected = [
        "Memory-stateFacts",
        "Memory-retractFacts",
        "Memory-listFacts",
        "Memory-search",
        "Memory-Schema-defineNamespace",
        "Memory-Schema-defineKind",
        "Memory-Schema-defineField",
        "Memory-getEntity",
        "Memory-createEntity",
    ];
    for name in expected {
        assert!(
            names.iter().any(|candidate| candidate == name),
            "missing tool spec `{}`",
            name
        );
    }
}

fn temp_paths(prefix: &str) -> (PathBuf, PathBuf) {
    let root = PathBuf::from(format!("/tmp/{}-{}", prefix, Uuid::now_v7()));
    let search = PathBuf::from(format!("/tmp/{}-search-{}", prefix, Uuid::now_v7()));
    (root, search)
}

async fn make_store(prefix: &str) -> MemoryStore {
    let (root, search) = temp_paths(prefix);
    let store = MemoryStore::new(root, search).expect("memory store");
    store.migrate().await.expect("memory migrate");
    store
}

fn request(tool_name: &str, arguments: Value) -> ToolRequest {
    ToolRequest {
        tool_call_id: format!("call_{}", Uuid::now_v7()),
        tool_name: tool_name.to_string(),
        arguments,
    }
}

fn unwrap_text_json(content: ToolResultData) -> Value {
    match content {
        ToolResultData::Text(text) => serde_json::from_str(&text).expect("json text payload"),
        other => panic!("expected text payload, got {:?}", other),
    }
}

fn sample_fact(entity: &str, field: &str, value: FactValue) -> FactInput {
    FactInput {
        source: Uri::parse(format!("borg:message:{}", Uuid::now_v7())).expect("source"),
        entity: Uri::parse(entity).expect("entity"),
        field: Uri::parse(field).expect("field"),
        arity: FactArity::One,
        value,
    }
}

#[tokio::test]
async fn rfd0005_current_state_facts_txid_is_server_generated_per_call() {
    let store = make_store("rfd0005-txid").await;
    let entity = format!("borg:person:{}", Uuid::now_v7());

    let call1 = store
        .state_facts(vec![sample_fact(
            &entity,
            "borg:field:displayName",
            FactValue::Text("mariana".to_string()),
        )])
        .await
        .expect("state_facts call1");

    let call2 = store
        .state_facts(vec![sample_fact(
            &entity,
            "borg:field:nickname",
            FactValue::Text("maya".to_string()),
        )])
        .await
        .expect("state_facts call2");

    assert_ne!(
        call1.tx_id.to_string(),
        call2.tx_id.to_string(),
        "every write call must produce a distinct txId"
    );
    assert!(call1.tx_id.to_string().starts_with("borg:tx:"));
    assert!(call2.tx_id.to_string().starts_with("borg:tx:"));
}

#[tokio::test]
async fn rfd0005_current_toolchain_search_memory_returns_json() {
    let store = make_store("rfd0005-search").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let response = tools
        .run(request(
            "Memory-searchMemory",
            json!({ "query": { "q": "nothing", "limit": 5 } }),
        ))
        .await
        .expect("searchMemory call");
    let body = unwrap_text_json(response.content);
    assert!(
        body.get("entities").is_some(),
        "search tool payload should include entities array"
    );
}

#[tokio::test]
async fn rfd0005_retraction_semantics() {
    let store = make_store("rfd0005-retract").await;
    let tools = build_memory_toolchain(store.clone()).expect("toolchain");
    let entity = format!("borg:person:{}", Uuid::now_v7());
    let source = format!("borg:message:{}", Uuid::now_v7());

    let write = tools
        .run(request(
            "Memory-stateFacts",
            json!({
                "source": source,
                "facts": [
                    {
                        "entity": entity,
                        "field": "borg:field:nickname",
                        "value": { "string": "maya" }
                    }
                ]
            }),
        ))
        .await
        .expect("stateFacts call");
    let write_body = unwrap_text_json(write.content);
    let fact_uri = write_body["factUris"][0].as_str().expect("fact uri");

    let retract = tools
        .run(request(
            "Memory-retractFacts",
            json!({
                "source": source,
                "targets": [{ "factUri": fact_uri, "reason": "test" }]
            }),
        ))
        .await
        .expect("retractFacts call");
    let retract_body = unwrap_text_json(retract.content);
    assert!(
        retract_body["retractionFactUris"].as_array().is_some(),
        "retraction output must include retraction fact URIs"
    );
}

#[tokio::test]
async fn rfd0005_warnings_contract() {
    let store = make_store("rfd0005-warnings").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let entity = format!("borg:person:{}", Uuid::now_v7());

    let response = tools
        .run(request("Memory-getEntity", json!({ "entityUri": entity })))
        .await
        .expect("getEntity call");
    let body = unwrap_text_json(response.content);
    let warnings = body["warnings"].as_array().expect("warnings array");
    for warning in warnings {
        assert!(warning.get("code").is_some(), "warning.code");
        assert!(warning.get("severity").is_some(), "warning.severity");
        assert!(warning.get("message").is_some(), "warning.message");
    }
}

#[tokio::test]
async fn rfd0005_same_as_identity_contract() {
    let store = make_store("rfd0005-sameas").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let user = format!("borg:user:{}", Uuid::now_v7());
    let source_uri = format!("spotify:artist:{}", Uuid::now_v7());
    let source = format!("borg:message:{}", Uuid::now_v7());

    let _ = tools
        .run(request(
            "Memory-stateFacts",
            json!({
                "source": source,
                "facts": [
                    { "entity": user, "field": "borg:field:sameAs", "value": { "uri": source_uri } }
                ]
            }),
        ))
        .await
        .expect("stateFacts sameAs call");

    let _ = tools
        .run(request(
            "Memory-search",
            json!({ "query": "artist", "resultTypes": ["entity"], "pagination": { "limit": 5 } }),
        ))
        .await
        .expect("search call");
}

#[tokio::test]
async fn rfd0005_same_as_prefers_borg_canonical_uri() {
    let store = make_store("rfd0005-sameas-canonical").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let canonical = format!("borg:person:{}", Uuid::now_v7());
    let duplicate = format!("spotify:artist:{}", Uuid::now_v7());
    let source = format!("borg:message:{}", Uuid::now_v7());

    let _ = tools
        .run(request(
            "Memory-stateFacts",
            json!({
                "source": source,
                "facts": [
                    { "entity": duplicate, "field": "borg:field:sameAs", "value": { "uri": canonical } }
                ]
            }),
        ))
        .await
        .expect("stateFacts sameAs call");

    let response = tools
        .run(request(
            "Memory-getEntity",
            json!({ "entityUri": duplicate }),
        ))
        .await
        .expect("getEntity call");
    let body = unwrap_text_json(response.content);
    let resolved = body["entityUri"].as_str().expect("canonical entityUri");
    assert!(
        resolved.starts_with("borg:"),
        "canonical sameAs resolution should prefer borg:* URIs"
    );
}

#[tokio::test]
async fn rfd0005_schema_tools_contract() {
    let store = make_store("rfd0005-schema").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let source = format!("borg:message:{}", Uuid::now_v7());

    let _ = tools
        .run(request(
            "Memory-Schema-defineNamespace",
            json!({
                "namespaceUri": "borg:namespace:spotify",
                "prefix": "spotify",
                "label": "Spotify",
                "source": source
            }),
        ))
        .await
        .expect("defineNamespace");

    let _ = tools
        .run(request(
            "Memory-Schema-defineField",
            json!({
                "fieldUri": "spotify:field:relatedTo",
                "domain": ["spotify:kind:artist"],
                "range": ["spotify:kind:artist"],
                "allowsMany": true,
                "isTransitive": false,
                "isReflexive": false,
                "isSymmetric": true,
                "source": source
            }),
        ))
        .await
        .expect("defineField");
}

#[tokio::test]
async fn rfd0005_value_schema_contract() {
    let store = make_store("rfd0005-values").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let entity = format!("borg:person:{}", Uuid::now_v7());
    let source = format!("borg:message:{}", Uuid::now_v7());

    let values = [
        json!({"string": "maya"}),
        json!({"number": 31}),
        json!({"bool": true}),
        json!({"date": "2026-02-28"}),
        json!({"datetime": "2026-02-28T12:34:56Z"}),
        json!({"uri": "borg:kind:person"}),
    ];

    for value in values {
        let _ = tools
            .run(request(
                "Memory-stateFacts",
                json!({
                    "source": source,
                    "facts": [{
                        "entity": entity,
                        "field": "borg:field:test",
                        "value": value
                    }]
                }),
            ))
            .await
            .expect("stateFacts typed value");
    }
}

#[tokio::test]
async fn rfd0005_state_facts_accepts_array_typed_values() {
    let store = make_store("rfd0005-array-values").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let entity = format!("borg:person:{}", Uuid::now_v7());
    let source = format!("borg:message:{}", Uuid::now_v7());

    let _ = tools
        .run(request(
            "Memory-stateFacts",
            json!({
                "source": source,
                "facts": [{
                    "entity": entity,
                    "field": "borg:field:alias",
                    "value": [
                        { "string": "mariana" },
                        { "string": "maya" }
                    ]
                }]
            }),
        ))
        .await
        .expect("stateFacts array value");

    let listed = tools
        .run(request(
            "Memory-listFacts",
            json!({
                "entity": entity,
                "field": "borg:field:alias",
                "includeRetracted": false,
                "pagination": { "limit": 10 }
            }),
        ))
        .await
        .expect("listFacts");
    let body = unwrap_text_json(listed.content);
    let first_value = &body["facts"][0]["value"];
    assert!(
        first_value.is_array(),
        "array typed values must roundtrip through listFacts"
    );
}

#[tokio::test]
async fn rfd0005_bootstrap_without_schema_mode() {
    let store = make_store("rfd0005-bootstrap").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let source = "borg:agent:bootstrap";

    let _ = tools
        .run(request(
            "Memory-stateFacts",
            json!({
                "source": source,
                "facts": [
                    { "entity": "borg:schema:field", "field": "borg:field:label", "value": { "string": "Field" } },
                    { "entity": "borg:field:label", "field": "borg:field:isA", "value": { "uri": "borg:schema:field" } }
                ]
            }),
        ))
        .await
        .expect("bootstrap write");
}

#[tokio::test]
async fn rfd0005_list_facts_contract() {
    let store = make_store("rfd0005-list").await;
    let tools = build_memory_toolchain(store.clone()).expect("toolchain");
    let entity = format!("borg:person:{}", Uuid::now_v7());
    let _ = store
        .state_facts(vec![sample_fact(
            &entity,
            "borg:field:displayName",
            FactValue::Text("mariana".to_string()),
        )])
        .await
        .expect("seed facts");

    let response = tools
        .run(request(
            "Memory-listFacts",
            json!({
                "entity": entity,
                "pagination": { "limit": 10 }
            }),
        ))
        .await
        .expect("listFacts");
    let body = unwrap_text_json(response.content);
    assert!(body.get("facts").is_some(), "facts");
    assert!(body.get("warnings").is_some(), "warnings");
}

#[tokio::test]
async fn rfd0005_search_contract() {
    let store = make_store("rfd0005-search-contract").await;
    let tools = build_memory_toolchain(store).expect("toolchain");
    let response = tools
        .run(request(
            "Memory-search",
            json!({
                "query": "mariana",
                "resultTypes": ["entity"],
                "pagination": { "limit": 10 }
            }),
        ))
        .await
        .expect("search");
    let body = unwrap_text_json(response.content);
    assert!(body.get("results").is_some(), "results");
}
