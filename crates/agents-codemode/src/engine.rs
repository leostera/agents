use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::config::CodeModeConfig;
use crate::native::NativeFunctionRegistry;
use crate::providers::{EnvironmentProvider, EnvironmentVariable, Package, PackageProvider};
use crate::request::{PackageMatch, Request, Response, RunCodeResult, SearchCodeResult};

/// Embeddable codemode engine.
#[derive(Clone)]
pub struct CodeMode {
    config: CodeModeConfig,
    package_providers: Vec<Arc<dyn PackageProvider>>,
    environment_providers: Vec<Arc<dyn EnvironmentProvider>>,
    native_functions: NativeFunctionRegistry,
}

/// Builder for [`CodeMode`].
pub struct CodeModeBuilder {
    config: CodeModeConfig,
    package_providers: Vec<Arc<dyn PackageProvider>>,
    environment_providers: Vec<Arc<dyn EnvironmentProvider>>,
    native_functions: NativeFunctionRegistry,
}

impl CodeMode {
    /// Starts building a new [`CodeMode`].
    pub fn builder() -> CodeModeBuilder {
        CodeModeBuilder {
            config: CodeModeConfig::default(),
            package_providers: Vec::new(),
            environment_providers: Vec::new(),
            native_functions: NativeFunctionRegistry::default(),
        }
    }

    /// Executes one codemode request.
    pub async fn execute(&self, request: Request) -> Result<Response> {
        match request {
            Request::RunCode(_request) => Err(anyhow!(
                "RunCode is not implemented yet; build the deno_core execution slice next"
            )),
            Request::SearchCode(request) => Ok(Response::SearchCode(
                self.search_code(request.query, request.limit).await?,
            )),
        }
    }

    /// Returns the engine configuration.
    pub fn config(&self) -> &CodeModeConfig {
        &self.config
    }

    pub(crate) fn native_function_names(&self) -> Vec<String> {
        self.native_functions.names()
    }

    pub(crate) async fn call_native_function(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.native_functions.call(name, args).await
    }

    async fn search_code(&self, query: String, limit: Option<usize>) -> Result<SearchCodeResult> {
        let query = query.trim();
        if query.is_empty() {
            return Err(anyhow!("SearchCode requires a non-empty query"));
        }

        let query_lower = query.to_lowercase();
        let limit = limit.unwrap_or_else(|| self.config.search_limit()).max(1);
        let packages = self.fetch_packages().await?;
        let mut matches = Vec::new();

        for package in packages {
            let package_name = package.name.to_lowercase();
            let snippet = snippet_for_query(&package.code, query);
            if package_name.contains(&query_lower) || snippet.is_some() {
                matches.push(PackageMatch {
                    name: package.name,
                    snippet,
                });
            }

            if matches.len() >= limit {
                break;
            }
        }

        Ok(SearchCodeResult { matches })
    }

    async fn fetch_packages(&self) -> Result<Vec<Package>> {
        let mut packages = Vec::new();
        for provider in &self.package_providers {
            packages.extend(provider.fetch().await?);
        }
        Ok(packages)
    }

    pub(crate) async fn fetch_environment(&self) -> Result<Vec<EnvironmentVariable>> {
        let mut vars = Vec::new();
        for provider in &self.environment_providers {
            vars.extend(provider.fetch().await?);
        }
        Ok(vars)
    }
}

impl CodeModeBuilder {
    /// Replaces the current configuration.
    pub fn with_config(mut self, config: CodeModeConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds one package provider.
    pub fn with_package_provider<P>(mut self, provider: P) -> Self
    where
        P: PackageProvider,
    {
        self.package_providers.push(Arc::new(provider));
        self
    }

    /// Adds one environment provider.
    pub fn with_environment_provider<P>(mut self, provider: P) -> Self
    where
        P: EnvironmentProvider,
    {
        self.environment_providers.push(Arc::new(provider));
        self
    }

    /// Merges in native functions exposed to JavaScript.
    pub fn with_native_functions(mut self, registry: NativeFunctionRegistry) -> Self {
        self.native_functions.merge_from(registry);
        self
    }

    /// Builds the engine.
    pub fn build(self) -> Result<CodeMode> {
        Ok(CodeMode {
            config: self.config,
            package_providers: self.package_providers,
            environment_providers: self.environment_providers,
            native_functions: self.native_functions,
        })
    }
}

fn snippet_for_query(code: &str, query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    code.lines()
        .map(str::trim)
        .find(|line| line.to_lowercase().contains(&query_lower))
        .map(ToString::to_string)
}

#[allow(dead_code)]
fn zero_duration() -> Duration {
    Duration::ZERO
}

#[allow(dead_code)]
fn empty_result() -> RunCodeResult {
    RunCodeResult {
        value: serde_json::Value::Null,
        duration: Duration::ZERO,
    }
}
