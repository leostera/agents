use serde_json::{Value, json};

use crate::CodeModeRuntime;

#[test]
fn executes_with_injected_sdk_and_ffi() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute("async () => { return Borg.OS.ls('.'); }")
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
        .execute("async () => { return Borg.OS.ls('a', 'b'); }")
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
        ))
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
fn execute_rejects_non_code_mode_shape() {
    let rt = CodeModeRuntime::default();
    let err = rt.execute("Borg.OS.ls('.')").unwrap_err();
    assert!(
        err.to_string().contains("async () =>"),
        "unexpected error: {err}"
    );
}

#[test]
fn search_returns_sdk_capabilities_from_types() {
    let rt = CodeModeRuntime::default();
    let results = rt.search("fetch");
    assert!(results.iter().any(|cap| cap.name == "Borg.fetch"));
}
