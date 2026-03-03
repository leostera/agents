use std::path::PathBuf;

use borg_agent::{ToolRequest, ToolResultData};
use borg_memory::{MemoryStore, build_memory_toolchain};
use proptest::prelude::*;
use proptest::string::string_regex;
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};
use uuid::Uuid;

fn token_strategy() -> BoxedStrategy<String> {
    string_regex("[a-z][a-z0-9_]{0,10}").unwrap().boxed()
}

fn namespace_strategy() -> BoxedStrategy<String> {
    prop_oneof![
        Just("borg".to_string()),
        Just("spotify".to_string()),
        Just("imdb".to_string()),
        Just("mem".to_string()),
    ]
    .boxed()
}

fn uri_strategy() -> BoxedStrategy<String> {
    (namespace_strategy(), token_strategy(), token_strategy())
        .prop_map(|(ns, kind, id)| format!("{ns}:{kind}:{id}"))
        .boxed()
}

fn typed_scalar_strategy() -> BoxedStrategy<Value> {
    let string_value = string_regex("[a-zA-Z0-9 _-]{1,16}")
        .unwrap()
        .prop_map(|v| json!({ "string": v }));
    let number_value = (-10_000i32..10_000i32).prop_map(|v| json!({ "number": v }));
    let bool_value = any::<bool>().prop_map(|v| json!({ "bool": v }));
    let date_value = (2000u16..2030, 1u8..13, 1u8..29)
        .prop_map(|(y, m, d)| json!({ "date": format!("{y:04}-{m:02}-{d:02}") }));
    let datetime_value = (2000u16..2030, 1u8..13, 1u8..29, 0u8..24, 0u8..60, 0u8..60)
        .prop_map(|(y, m, d, hh, mm, ss)| {
            json!({ "datetime": format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z") })
        });
    let uri_value = uri_strategy().prop_map(|v| json!({ "uri": v }));
    prop_oneof![
        string_value,
        number_value,
        bool_value,
        date_value,
        datetime_value,
        uri_value
    ]
    .boxed()
}

fn typed_scalar_no_number_strategy() -> BoxedStrategy<Value> {
    let string_value = string_regex("[a-zA-Z0-9 _-]{1,16}")
        .unwrap()
        .prop_map(|v| json!({ "string": v }));
    let bool_value = any::<bool>().prop_map(|v| json!({ "bool": v }));
    let date_value = (2000u16..2030, 1u8..13, 1u8..29)
        .prop_map(|(y, m, d)| json!({ "date": format!("{y:04}-{m:02}-{d:02}") }));
    let datetime_value = (2000u16..2030, 1u8..13, 1u8..29, 0u8..24, 0u8..60, 0u8..60)
        .prop_map(|(y, m, d, hh, mm, ss)| {
            json!({ "datetime": format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z") })
        });
    let uri_value = uri_strategy().prop_map(|v| json!({ "uri": v }));
    prop_oneof![
        string_value,
        bool_value,
        date_value,
        datetime_value,
        uri_value
    ]
    .boxed()
}

fn typed_value_strategy() -> BoxedStrategy<Value> {
    let scalar = typed_scalar_strategy();
    let list = prop::collection::vec(typed_scalar_strategy(), 1..4).prop_map(Value::Array);
    prop_oneof![scalar, list].boxed()
}

fn typed_value_no_number_strategy() -> BoxedStrategy<Value> {
    let scalar = typed_scalar_no_number_strategy();
    let list =
        prop::collection::vec(typed_scalar_no_number_strategy(), 1..4).prop_map(Value::Array);
    prop_oneof![scalar, list].boxed()
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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 20,
        .. ProptestConfig::default()
    })]

    #[test]
    fn prop_generated_fact_is_searchable(
        entity in uri_strategy(),
        field in uri_strategy(),
        value in typed_value_strategy(),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async move {
            let store = make_store("rfd0005-prop-searchable").await;
            let tools = build_memory_toolchain(store).expect("toolchain");
            let source = format!("borg:message:{}", Uuid::now_v7());

            let write = tools
                .run(request(
                    "Memory-stateFacts",
                    json!({
                        "source": source,
                        "facts": [{ "entity": entity.clone(), "field": field, "value": value }]
                    }),
                ))
                .await
                .expect("stateFacts");
            let write_body = unwrap_text_json(write.content);
            prop_assert!(write_body["txId"].as_str().unwrap_or_default().starts_with("borg:tx:"));

            let mut found = false;
            for _ in 0..40 {
                let search = tools
                    .run(request(
                        "Memory-search",
                        json!({ "query": entity, "resultTypes": ["entity"], "pagination": { "limit": 20 } }),
                    ))
                    .await
                    .expect("search");
                let body = unwrap_text_json(search.content);
                found = body["results"].as_array().map(|items| items.iter()).into_iter().flatten()
                    .any(|item| item.get("uri").and_then(Value::as_str) == Some(entity.as_str()));
                if found {
                    break;
                }
                sleep(Duration::from_millis(25)).await;
            }
            prop_assert!(found, "entity should become searchable by its URI");
            Ok::<(), proptest::test_runner::TestCaseError>(())
        })?;
    }

    #[test]
    fn prop_entity_field_fact_is_listed_in_get_entity(
        entity in uri_strategy(),
        field in uri_strategy(),
        value in typed_value_strategy(),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async move {
            let store = make_store("rfd0005-prop-getentity").await;
            let tools = build_memory_toolchain(store).expect("toolchain");
            let source = format!("borg:message:{}", Uuid::now_v7());

            let _ = tools
                .run(request(
                    "Memory-stateFacts",
                    json!({
                        "source": source,
                        "facts": [{ "entity": entity.clone(), "field": field.clone(), "value": value }]
                    }),
                ))
                .await
                .expect("stateFacts");

            let response = tools
                .run(request(
                    "Memory-getEntity",
                    json!({ "entityUri": entity.clone(), "factPagination": { "limit": 100 } }),
                ))
                .await
                .expect("getEntity");
            let body = unwrap_text_json(response.content);
            let has_field = body["facts"].as_array().map(|items| items.iter()).into_iter().flatten()
                .any(|fact| {
                    fact.get("entity").and_then(Value::as_str) == Some(entity.as_str())
                        && fact.get("field").and_then(Value::as_str) == Some(field.as_str())
                });
            prop_assert!(has_field, "getEntity should include newly stated entity+field facts");
            Ok::<(), proptest::test_runner::TestCaseError>(())
        })?;
    }

    #[test]
    fn prop_retract_pattern_matches_exact_typed_value_only(
        entity in uri_strategy(),
        field in uri_strategy(),
        value_a in typed_value_no_number_strategy(),
        value_b in typed_value_no_number_strategy(),
    ) {
        prop_assume!(value_a != value_b);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async move {
            let store = make_store("rfd0005-prop-retract").await;
            let tools = build_memory_toolchain(store).expect("toolchain");
            let source = format!("borg:message:{}", Uuid::now_v7());

            let _ = tools
                .run(request(
                    "Memory-stateFacts",
                    json!({
                        "source": source,
                        "facts": [
                            { "entity": entity.clone(), "field": field.clone(), "value": value_a.clone() },
                            { "entity": entity.clone(), "field": field.clone(), "value": value_b.clone() }
                        ]
                    }),
                ))
                .await
                .expect("stateFacts");

            let _ = tools
                .run(request(
                    "Memory-retractFacts",
                    json!({
                        "source": source,
                        "targets": [{
                            "pattern": {
                                "entity": entity.clone(),
                                "field": field.clone(),
                                "value": value_a.clone()
                            }
                        }]
                    }),
                ))
                .await
                .expect("retractFacts");

            let listed = tools
                .run(request(
                    "Memory-listFacts",
                    json!({
                        "entity": entity.clone(),
                        "field": field.clone(),
                        "includeRetracted": true,
                        "pagination": { "limit": 200 }
                    }),
                ))
                .await
                .expect("listFacts");
            let body = unwrap_text_json(listed.content);
            let facts = body["facts"].as_array().expect("facts array");

            let all_a_retracted = facts.iter().filter(|fact| fact.get("value") == Some(&value_a)).all(|fact| {
                fact.get("isRetracted").and_then(Value::as_bool) == Some(true)
            });
            let any_b_active = facts.iter().any(|fact| {
                fact.get("value") == Some(&value_b)
                    && fact.get("isRetracted").and_then(Value::as_bool) == Some(false)
            });
            prop_assert!(all_a_retracted, "exact-matched value should be retracted");
            prop_assert!(any_b_active, "non-matching value should remain active");
            Ok::<(), proptest::test_runner::TestCaseError>(())
        })?;
    }

    #[test]
    fn prop_same_as_transitive_prefers_borg_canonical(
        a_kind in token_strategy(),
        a_id in token_strategy(),
        b_kind in token_strategy(),
        b_id in token_strategy(),
        borg_kind in token_strategy(),
        borg_id in token_strategy(),
    ) {
        let a = format!("spotify:{a_kind}:{a_id}");
        let b = format!("imdb:{b_kind}:{b_id}");
        let canonical = format!("borg:{borg_kind}:{borg_id}");
        prop_assume!(a != b && b != canonical && a != canonical);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async move {
            let store = make_store("rfd0005-prop-sameas").await;
            let tools = build_memory_toolchain(store).expect("toolchain");
            let source = format!("borg:message:{}", Uuid::now_v7());

            let _ = tools
                .run(request(
                    "Memory-stateFacts",
                    json!({
                        "source": source,
                        "facts": [
                            { "entity": a.clone(), "field": "borg:field:sameAs", "value": { "uri": b.clone() } },
                            { "entity": b.clone(), "field": "borg:field:sameAs", "value": { "uri": canonical.clone() } }
                        ]
                    }),
                ))
                .await
                .expect("stateFacts sameAs");

            let response = tools
                .run(request(
                    "Memory-getEntity",
                    json!({ "entityUri": a.clone() }),
                ))
                .await
                .expect("getEntity");
            let body = unwrap_text_json(response.content);
            let resolved = body["entityUri"].as_str().unwrap_or_default();
            prop_assert!(resolved.starts_with("borg:"), "canonical sameAs URI should prefer borg:*");
            let closure = body["sameAs"].as_array().expect("sameAs closure");
            prop_assert!(closure.iter().any(|u| u.as_str() == Some(canonical.as_str())));
            prop_assert!(closure.iter().any(|u| u.as_str() == Some(a.as_str())));
            prop_assert!(closure.iter().any(|u| u.as_str() == Some(b.as_str())));
            Ok::<(), proptest::test_runner::TestCaseError>(())
        })?;
    }
}
