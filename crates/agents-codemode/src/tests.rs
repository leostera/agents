use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use crate::{
    CodeMode, CodeModeConfig, EnvironmentProvider, EnvironmentVariable, NativeFunctionRegistry,
    Package, PackageProvider, Request, Response, SearchCode,
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
        .execute(Request::SearchCode(SearchCode {
            query: "fetch".to_string(),
            limit: Some(5),
        }))
        .await
        .expect("search succeeds");

    match response {
        Response::SearchCode(result) => {
            assert_eq!(result.matches.len(), 1);
            assert_eq!(result.matches[0].name, "@scope/fetch-tools");
            assert_eq!(
                result.matches[0].snippet.as_deref(),
                Some("export const fetchJson = () => fetch('/api');")
            );
        }
        Response::RunCode(_) => panic!("expected search response"),
    }
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
        .execute(Request::SearchCode(SearchCode {
            query: "matchme".to_string(),
            limit: None,
        }))
        .await
        .expect("search succeeds");

    match response {
        Response::SearchCode(result) => assert_eq!(result.matches.len(), 2),
        Response::RunCode(_) => panic!("expected search response"),
    }
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
async fn run_code_is_a_typed_stub_until_deno_core_execution_is_added() {
    let codemode = CodeMode::builder().build().expect("codemode");
    let error = codemode
        .execute(crate::Request::RunCode(crate::RunCode {
            code: "async () => ({ ok: true })".to_string(),
            imports: Vec::new(),
        }))
        .await
        .expect_err("run code not implemented yet");

    assert!(error.to_string().contains("RunCode is not implemented yet"));
}
