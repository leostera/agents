use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use crate::{
    CodeMode, CodeModeConfig, EnvironmentProvider, EnvironmentVariable, NativeFunctionRegistry,
    Package, PackageProvider, RunCode, SearchCode,
};

#[derive(Clone)]
struct StaticPackageProvider(Vec<Package>);

#[async_trait]
impl PackageProvider for StaticPackageProvider {
    async fn fetch(&self) -> Result<Vec<Package>> {
        Ok(self.0.clone())
    }
}

#[derive(Clone)]
struct StaticEnvironmentProvider(Vec<EnvironmentVariable>);

#[async_trait]
impl EnvironmentProvider for StaticEnvironmentProvider {
    async fn fetch(&self) -> Result<Vec<EnvironmentVariable>> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn search_code_matches_package_names_and_snippets() {
    let codemode = CodeMode::builder()
        .with_config(CodeModeConfig::default().default_search_limit(10))
        .with_package_provider(StaticPackageProvider(vec![
            Package {
                name: "@scope/fetch-tools".to_string(),
                code: "export const fetchJson = () => fetch('/api');".to_string(),
            },
            Package {
                name: "@scope/math".to_string(),
                code: "export const add = (a, b) => a + b;".to_string(),
            },
        ]))
        .build()
        .expect("codemode");

    let response = codemode
        .search_code(SearchCode {
            query: "fetch".to_string(),
            limit: Some(5),
        })
        .await
        .expect("search succeeds");

    assert_eq!(response.matches.len(), 1);
    assert_eq!(response.matches[0].name, "@scope/fetch-tools");
    assert_eq!(
        response.matches[0].snippet.as_deref(),
        Some("export const fetchJson = () => fetch('/api');")
    );
}

#[tokio::test]
async fn search_code_uses_default_search_limit() {
    let packages = (0..5)
        .map(|index| Package {
            name: format!("pkg-{index}"),
            code: "export const matchMe = true;".to_string(),
        })
        .collect::<Vec<_>>();

    let codemode = CodeMode::builder()
        .with_config(CodeModeConfig::default().default_search_limit(2))
        .with_package_provider(StaticPackageProvider(packages))
        .build()
        .expect("codemode");

    let response = codemode
        .search_code(SearchCode {
            query: "matchme".to_string(),
            limit: None,
        })
        .await
        .expect("search succeeds");

    assert_eq!(response.matches.len(), 2);
}

#[tokio::test]
async fn fetch_environment_aggregates_all_providers() {
    let codemode = CodeMode::builder()
        .with_environment_provider(StaticEnvironmentProvider(vec![EnvironmentVariable {
            name: "API_KEY".to_string(),
            value: "123".to_string(),
        }]))
        .with_environment_provider(StaticEnvironmentProvider(vec![EnvironmentVariable {
            name: "BASE_URL".to_string(),
            value: "http://localhost".to_string(),
        }]))
        .build()
        .expect("codemode");

    let env = codemode.fetch_environment().await.expect("env");
    assert_eq!(env.len(), 2);
    assert!(env.iter().any(|value| value.name == "API_KEY"));
    assert!(env.iter().any(|value| value.name == "BASE_URL"));
}

#[tokio::test]
async fn native_function_registry_can_be_called_through_codemode() {
    let codemode = CodeMode::builder()
        .with_native_functions(NativeFunctionRegistry::default().add_function(
            "sum",
            |args: serde_json::Value| async move {
                let numbers = args
                    .as_array()
                    .expect("array")
                    .iter()
                    .filter_map(|value: &serde_json::Value| value.as_i64())
                    .sum::<i64>();
                Ok(json!({ "sum": numbers }))
            },
        ))
        .build()
        .expect("codemode");

    let response = codemode
        .call_native_function("sum", json!([1, 2, 3]))
        .await
        .expect("native call");
    assert_eq!(response, json!({ "sum": 6 }));
    assert_eq!(codemode.native_function_names(), vec!["sum".to_string()]);
}

#[tokio::test]
async fn run_code_executes_async_zero_arg_closure() {
    let codemode = CodeMode::builder().build().expect("codemode");
    let response = codemode
        .run_code(RunCode {
            code: "async () => ({ ok: true, value: 42 })".to_string(),
            imports: Vec::new(),
        })
        .await
        .expect("run code");

    assert_eq!(response.value, json!({ "ok": true, "value": 42 }));
}

#[tokio::test]
async fn run_code_imports_packages_from_package_providers() {
    let codemode = CodeMode::builder()
        .with_package_provider(StaticPackageProvider(vec![Package {
            name: "@scope/math".to_string(),
            code: "export const add = (a, b) => a + b;".to_string(),
        }]))
        .build()
        .expect("codemode");

    let response = codemode
        .run_code(RunCode {
            code:
                "async () => { const math = await import('@scope/math'); return math.add(2, 3); }"
                    .to_string(),
            imports: vec!["@scope/math".to_string()],
        })
        .await
        .expect("run code");

    assert_eq!(response.value, json!(5));
}

#[tokio::test]
async fn run_code_exposes_env_globals() {
    let codemode = CodeMode::builder()
        .with_environment_provider(StaticEnvironmentProvider(vec![EnvironmentVariable {
            name: "API_KEY".to_string(),
            value: "secret".to_string(),
        }]))
        .build()
        .expect("codemode");

    let response = codemode
        .run_code(RunCode {
            code: "async () => { return { keys: await env.keys(), key: await env.get('API_KEY'), missing: await env.get('MISSING', 'fallback') }; }".to_string(),
            imports: Vec::new(),
        })
        .await
        .expect("run code");

    assert_eq!(response.value.get("key"), Some(&json!("secret")));
    assert_eq!(response.value.get("missing"), Some(&json!("fallback")));
    let keys = response
        .value
        .get("keys")
        .and_then(serde_json::Value::as_array)
        .expect("keys array");
    assert!(keys.iter().any(|value| value.as_str() == Some("API_KEY")));
}

#[tokio::test]
async fn run_code_calls_user_native_functions() {
    let codemode = CodeMode::builder()
        .with_native_functions(NativeFunctionRegistry::default().add_function(
            "sum",
            |args: serde_json::Value| async move {
                let numbers = args
                    .as_array()
                    .expect("array")
                    .iter()
                    .filter_map(|value: &serde_json::Value| value.as_i64())
                    .sum::<i64>();
                Ok(json!({ "sum": numbers }))
            },
        ))
        .build()
        .expect("codemode");

    let response = codemode
        .run_code(RunCode {
            code: "async () => { return await native.sum(1, 2, 3); }".to_string(),
            imports: Vec::new(),
        })
        .await
        .expect("run code");

    assert_eq!(response.value, json!({ "sum": 6 }));
}

#[tokio::test]
async fn run_code_exposes_built_in_fetch() {
    let server = spawn_json_server(r#"{"ok":true,"source":"test"}"#);
    let codemode = CodeMode::builder().build().expect("codemode");

    let response = codemode
        .run_code(RunCode {
            code: format!(
                "async () => {{ return await fetch('{}', {{ method: 'GET' }}); }}",
                server
            ),
            imports: Vec::new(),
        })
        .await
        .expect("run code");

    assert_eq!(response.value.get("ok"), Some(&json!(true)));
    assert_eq!(response.value.get("status"), Some(&json!(200)));
    assert_eq!(
        response
            .value
            .get("json")
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("source")),
        Some(&json!("test"))
    );
}

fn spawn_json_server(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("server address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });
    format!("http://{}", address)
}
