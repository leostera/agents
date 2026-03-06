use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{CodeModeContext, CodeModeRuntime};

#[test]
fn executes_with_injected_sdk_and_ffi() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute(
            "async () => { return Borg.OS.ls('.'); }",
            CodeModeContext::default(),
        )
        .unwrap();
    assert!(result.result.is_object());
    let entries = result
        .result
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
        .execute(
            "async () => { return Borg.OS.ls('a', 'b'); }",
            CodeModeContext::default(),
        )
        .unwrap();
    assert_eq!(result.result, json!({ "args": ["a", "b"] }));
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

    assert_eq!(result.result.get("status"), Some(&json!(200)));
    assert_eq!(result.result.get("ok"), Some(&json!(true)));
    assert_eq!(
        result
            .result
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
    assert_eq!(result.result, json!("borg:user:leostera"));
}

#[test]
fn env_get_and_keys_are_available_via_sdk() {
    let rt = CodeModeRuntime::default();
    let mut env = HashMap::new();
    env.insert("GITHUB_ACCESS_TOKEN".to_string(), "token-123".to_string());
    env.insert("GITHUB_SCOPE".to_string(), "read:user".to_string());
    let result = rt
        .execute(
            "async () => { return { keys: Borg.env.keys(), token: Borg.env.get('GITHUB_ACCESS_TOKEN'), missing: Borg.env.get('GITHUB_MISSING', 'fallback') }; }",
            CodeModeContext {
                env,
                ..CodeModeContext::default()
            },
        )
        .unwrap();

    let keys = result
        .result
        .get("keys")
        .and_then(Value::as_array)
        .expect("keys array");
    assert!(
        keys.iter()
            .any(|value| value.as_str() == Some("GITHUB_ACCESS_TOKEN"))
    );
    assert_eq!(
        result.result.get("token").and_then(Value::as_str),
        Some("token-123")
    );
    assert_eq!(
        result.result.get("missing").and_then(Value::as_str),
        Some("fallback")
    );
}

#[test]
fn context_current_exposes_only_env_keys_not_values() {
    let rt = CodeModeRuntime::default();
    let mut env = HashMap::new();
    env.insert(
        "GITHUB_ACCESS_TOKEN".to_string(),
        "super-secret-token".to_string(),
    );
    let result = rt
        .execute(
            "async () => { return ffi('context__current', []); }",
            CodeModeContext {
                env,
                ..CodeModeContext::default()
            },
        )
        .unwrap();
    assert!(result.result.get("env").is_none());
    let available = result
        .result
        .get("available_env_keys")
        .and_then(Value::as_array)
        .expect("available env keys");
    assert!(
        available
            .iter()
            .any(|value| value.as_str() == Some("GITHUB_ACCESS_TOKEN"))
    );
}

#[test]
fn execute_rejects_non_code_mode_shape() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute("Borg.OS.ls('.')", CodeModeContext::default())
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("async zero-arg function expression"),
        "unexpected error: {err}"
    );
}

#[test]
fn execute_invalid_borgos_symbol_returns_corrective_hint() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute(
            "async () => { return BorgOs.ls('.'); }",
            CodeModeContext::default(),
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("Borg.OS.ls"),
        "unexpected error message: {message}"
    );
}

#[test]
fn execute_surfaces_error_for_invalid_borg_memory_namespace() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute(
            "async () => { const memoryStorage = Borg.LTM; await memoryStorage.store('leo', 'realName', 'leandro'); return 'ok'; }",
            CodeModeContext::default(),
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(!message.is_empty(), "unexpected empty error");
}

#[test]
fn execute_rejects_borg_memory_namespace() {
    let rt = CodeModeRuntime::default();
    let err = rt
        .execute(
            "async () => { return Borg.Memory.stateFacts([{ source: 'borg:message:abc', entity: 'borg:user:leo', field: 'borg:field:real_name', value: { Text: 'Leandro' } }]); }",
            CodeModeContext::default(),
        )
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn execute_allows_valid_borg_me_calls() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute(
            "async () => { return Borg.me().uri(); }",
            CodeModeContext::default(),
        )
        .unwrap();
    assert_eq!(result.result, Value::Null);
}

#[test]
fn ffi_handler_panic_is_reported_as_runtime_error() {
    let rt = CodeModeRuntime::default().with_ffi_handler("os__ls", |_args| {
        panic!("simulated ffi panic");
    });
    let err = rt
        .execute(
            "async () => { return Borg.OS.ls('.'); }",
            CodeModeContext::default(),
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("ffi execution panic"),
        "unexpected error: {err}"
    );
}

#[test]
fn execute_supports_dynamic_file_imports() {
    let rt = CodeModeRuntime::default();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    let module_path = std::env::temp_dir().join(format!("borg-codemode-import-{unique}.mjs"));
    std::fs::write(&module_path, "export const dynamicValue = 42;").expect("write temp module");
    let module_specifier = deno_core::ModuleSpecifier::from_file_path(&module_path)
        .expect("convert temp module path to file specifier")
        .to_string();
    let code = format!(
        "async () => {{ const m = await import('{module_specifier}'); return m.dynamicValue; }}"
    );

    let result = rt.execute(&code, CodeModeContext::default()).unwrap();
    assert_eq!(result.result, json!(42));

    let _ = std::fs::remove_file(module_path);
}

#[test]
fn execute_supports_static_npm_imports_inside_module() {
    let rt = CodeModeRuntime::default();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    let module_path =
        std::env::temp_dir().join(format!("borg-codemode-static-import-{unique}.mjs"));
    let module_source = r#"
import kleur from "npm:kleur@4.1.5";
export default async () => kleur.bold("ok");
"#;
    std::fs::write(&module_path, module_source).expect("write temp module");
    let module_specifier = deno_core::ModuleSpecifier::from_file_path(&module_path)
        .expect("convert temp module path to file specifier")
        .to_string();
    let code = format!(
        "async () => {{ const mod = await import('{module_specifier}'); return await mod.default(); }}"
    );

    let result = rt.execute(&code, CodeModeContext::default()).unwrap();
    assert!(
        result
            .result
            .as_str()
            .is_some_and(|value| value.contains("ok"))
    );

    let _ = std::fs::remove_file(module_path);
}

#[test]
fn execute_supports_dynamic_jsr_imports() {
    let rt = CodeModeRuntime::default();
    let result = rt
        .execute(
            "async () => { const semver = await import('jsr:@std/semver@1.0.0'); const a = semver.parse('1.2.0'); const b = semver.parse('1.1.0'); return semver.compare(a, b); }",
            CodeModeContext::default(),
        )
        .unwrap();
    assert_eq!(result.result, json!(1));
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
