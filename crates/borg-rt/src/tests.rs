use serde_json::{Value, json};

use crate::{CodeModeContext, CodeModeRuntime};

#[test]
fn executes_with_injected_sdk_and_ffi() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute("async () => { return Borg.OS.ls('.'); }", CodeModeContext::default())
        .unwrap();
    assert!(result.result_json.is_object());
    let entries = result
        .result_json
        .get("entries")
        .and_then(Value::as_array)
        .unwrap();
    assert!(!entries.is_empty());
}

#[test]
fn custom_ffi_handler_can_override_sdk_behavior() {
    let rt =
        CodeModeRuntime::default().with_ffi_handler("os__ls", |args| Ok(json!({ "args": args })));
    let result = rt
        .execute("async () => { return Borg.OS.ls('a', 'b'); }", CodeModeContext::default())
        .unwrap();
    assert_eq!(result.result_json, json!({ "args": ["a", "b"] }));
}

#[test]
fn fetch_uses_net_ffi_and_returns_response_shape() {
    let rt = CodeModeRuntime::default().with_ffi_handler("net__fetch", |args| {
        Ok(json!({
            "ok": true,
            "status": 200,
            "status_text": "OK",
            "url": args.first().and_then(Value::as_str).unwrap_or_default(),
            "headers": { "content-type": "application/json" },
            "body": r#"{"ok":true,"source":"test-server"}"#,
            "json": { "ok": true, "source": "test-server" }
        }))
    });
    let result = rt
        .execute(&format!(
            "async () => {{ return Borg.fetch('{}', {{ method: 'GET', headers: {{ 'x-test': '1' }} }}); }}",
            "http://example.test/hello"
        ), CodeModeContext::default())
        .unwrap();

    assert_eq!(result.result_json.get("status"), Some(&json!(200)));
    assert_eq!(result.result_json.get("ok"), Some(&json!(true)));
    assert_eq!(
        result
            .result_json
            .get("json")
            .and_then(|v| v.get("source"))
            .and_then(Value::as_str),
        Some("test-server")
    );
}

#[test]
fn me_returns_current_user_uri_from_context() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute(
            "async () => { return Borg.me().uri(); }",
            CodeModeContext {
                current_user_id: Some(
                    borg_core::Uri::parse("borg:user:leostera").expect("valid uri"),
                ),
                ..CodeModeContext::default()
            },
        )
        .unwrap();
    assert_eq!(result.result_json, json!("borg:user:leostera"));
}

#[test]
fn execute_rejects_non_code_mode_shape() {
    let rt = CodeModeRuntime::default();
    let err = rt.execute("Borg.OS.ls('.')", CodeModeContext::default()).unwrap_err();
    assert!(
        err.to_string().contains("async () =>"),
        "unexpected error: {err}"
    );
}

#[test]
fn execute_invalid_borgos_symbol_returns_corrective_hint() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute("async () => { return BorgOs.ls('.'); }", CodeModeContext::default())
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("Borg.OS.ls"),
        "unexpected error message: {message}"
    );
}

#[test]
fn execute_rejects_invalid_borg_ltm_namespace_before_runtime() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute(
            "async () => { const memoryStorage = Borg.LTM; await memoryStorage.store('leo', 'realName', 'leandro'); return 'ok'; }",
            CodeModeContext::default(),
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("Borg.LTM") && message.contains("Borg.Memory"),
        "unexpected error: {message}"
    );
}

#[test]
fn execute_allows_valid_borg_memory_calls() {
    let rt = CodeModeRuntime::default().with_ffi_handler("memory__state_facts", |_args| {
        Ok(json!({
            "tx_id": "borg:tx:test",
            "facts": []
        }))
    });
    let result = rt
        .execute(
            "async () => { return Borg.Memory.stateFacts([{ source: 'borg:message:abc', entity: 'borg:user:leo', field: 'borg:field:real_name', value: { Text: 'Leandro' } }]); }",
            CodeModeContext::default(),
        )
        .unwrap();
    assert_eq!(result.result_json.get("tx_id"), Some(&json!("borg:tx:test")));
}

#[test]
fn execute_allows_valid_borg_me_calls() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute("async () => { return Borg.me().uri(); }", CodeModeContext::default())
        .unwrap();
    assert_eq!(result.result_json, Value::Null);
}

#[test]
fn ffi_handler_panic_is_reported_as_runtime_error() {
    let rt = CodeModeRuntime::default().with_ffi_handler("os__ls", |_args| {
        panic!("simulated ffi panic");
    });
    let err = rt
        .execute("async () => { return Borg.OS.ls('.'); }", CodeModeContext::default())
        .unwrap_err();
    assert!(
        err.to_string().contains("ffi execution panic"),
        "unexpected error: {err}"
    );
}

#[test]
fn search_returns_sdk_capabilities_from_types() {
    let rt = CodeModeRuntime::default();
    let fetch_results = rt.search("fetch");
    let fetch = fetch_results
        .iter()
        .find(|cap| cap.name == "Borg.fetch")
        .expect("expected Borg.fetch capability");
    assert_eq!(fetch.symbol, "fetch");
    assert!(fetch.signature.contains("(url: string"));
    assert!(fetch.signature.contains("BorgFetchResponse"));
    assert!(fetch.type_definition.contains("type Fn ="));
    assert!(
        fetch
            .type_definition
            .contains("interface BorgFetchResponse")
    );

    let ls_results = rt.search("ls");
    let ls = ls_results
        .iter()
        .find(|cap| cap.name == "Borg.OS.ls")
        .expect("expected Borg.OS.ls capability");
    assert_eq!(ls.symbol, "OS.ls");
    assert!(ls.signature.contains("(path?: PathLike"));
    assert!(ls.signature.contains("BorgLsResult"));
    assert!(ls.type_definition.contains("interface BorgLsOptions"));
    assert!(ls.type_definition.contains("interface BorgLsResult"));
}
