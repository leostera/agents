use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use async_trait::async_trait;
use codemode::{
    CodeMode, CodeModeConfig, CodeModeResult, EnvironmentProvider, EnvironmentVariable,
    NativeFunctionRegistry, Package, PackageProvider, RunCode, SearchCode,
};
use serde_json::json;

#[derive(Clone)]
struct StaticPackageProvider(Vec<Package>);

#[async_trait]
impl PackageProvider for StaticPackageProvider {
    async fn fetch(&self) -> CodeModeResult<Vec<Package>> {
        Ok(self.0.clone())
    }
}

#[derive(Clone)]
struct StaticEnvironmentProvider(Vec<EnvironmentVariable>);

#[async_trait]
impl EnvironmentProvider for StaticEnvironmentProvider {
    async fn fetch(&self) -> CodeModeResult<Vec<EnvironmentVariable>> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn run_code_supports_public_engine_surface_end_to_end() {
    let server = spawn_json_server(r#"{"source":"engine-e2e"}"#);
    let codemode = CodeMode::builder()
        .with_config(CodeModeConfig::default().multithreaded(true))
        .with_package_provider(StaticPackageProvider(vec![Package {
            name: "@scope/math".to_string(),
            code: "export const add = (a, b) => a + b;".to_string(),
        }]))
        .with_environment_provider(StaticEnvironmentProvider(vec![EnvironmentVariable {
            name: "TOKEN".to_string(),
            value: "secret-token".to_string(),
        }]))
        .with_native_functions(NativeFunctionRegistry::default().add_function(
            "sum",
            |args: serde_json::Value| async move {
                let total = args
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_i64)
                    .sum::<i64>();
                Ok(json!({ "total": total }))
            },
        ))
        .build()
        .expect("codemode");

    let result = codemode
        .run_code(RunCode {
            code: format!(
                "async () => {{
                    const math = await import('@scope/math');
                    const fetched = await fetch('{server}');
                    return {{
                        total: math.add(2, 5),
                        envToken: await env.get('TOKEN'),
                        nativeTotal: (await native.sum(1, 2, 3)).total,
                        fetchedSource: fetched.json.source,
                    }};
                }}"
            ),
            imports: vec!["@scope/math".to_string()],
        })
        .await
        .expect("run code");

    assert_eq!(
        result.value,
        json!({
            "total": 7,
            "envToken": "secret-token",
            "nativeTotal": 6,
            "fetchedSource": "engine-e2e",
        })
    );
}

#[tokio::test]
async fn search_code_supports_public_engine_surface_end_to_end() {
    let codemode = CodeMode::builder()
        .with_package_provider(StaticPackageProvider(vec![
            Package {
                name: "@scope/http".to_string(),
                code: "export async function fetchJson(url) { return fetch(url); }".to_string(),
            },
            Package {
                name: "@scope/math".to_string(),
                code: "export const mul = (a, b) => a * b;".to_string(),
            },
        ]))
        .build()
        .expect("codemode");

    let result = codemode
        .search_code(SearchCode {
            query: "fetch".to_string(),
            limit: Some(10),
        })
        .await
        .expect("search code");

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].name, "@scope/http");
    assert_eq!(
        result.matches[0].snippet.as_deref(),
        Some("export async function fetchJson(url) { return fetch(url); }")
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
