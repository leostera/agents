use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use deno_core::{JsRuntime, PollEventLoopOptions, RuntimeOptions, serde_v8, v8};

use crate::config::CodeModeConfig;
use crate::error::{CodeModeError, CodeModeResult};
use crate::host::{extension as host_extension, install_host_functions};
use crate::module_loader::CodeModeModuleLoader;
use crate::native::NativeFunctionRegistry;
use crate::providers::{EnvironmentProvider, EnvironmentVariable, Package, PackageProvider};
use crate::request::{PackageMatch, RunCode, RunCodeResult, SearchCode, SearchCodeResult};

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

    /// Returns the engine configuration.
    pub fn config(&self) -> &CodeModeConfig {
        &self.config
    }

    #[cfg(test)]
    pub(crate) fn native_function_names(&self) -> Vec<String> {
        self.native_functions.names()
    }

    #[cfg(test)]
    pub(crate) async fn call_native_function(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> CodeModeResult<serde_json::Value> {
        self.native_functions.call(name, args).await
    }

    /// Searches package names and package source snippets exposed by package providers.
    pub async fn search_code(&self, request: SearchCode) -> CodeModeResult<SearchCodeResult> {
        let query = request.query.trim();
        if query.is_empty() {
            return Err(CodeModeError::EmptySearchQuery);
        }

        let query_lower = query.to_lowercase();
        let limit = request
            .limit
            .unwrap_or_else(|| self.config.search_limit())
            .max(1);
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

    async fn fetch_packages(&self) -> CodeModeResult<Vec<Package>> {
        let mut packages = Vec::new();
        for provider in &self.package_providers {
            packages.extend(provider.fetch().await?);
        }
        Ok(packages)
    }

    #[cfg(test)]
    pub(crate) async fn fetch_environment(&self) -> CodeModeResult<Vec<EnvironmentVariable>> {
        let mut vars = Vec::new();
        for provider in &self.environment_providers {
            vars.extend(provider.fetch().await?);
        }
        Ok(vars)
    }

    /// Executes one JavaScript async zero-argument closure inside the codemode isolate.
    pub async fn run_code(&self, request: RunCode) -> CodeModeResult<RunCodeResult> {
        let packages = self.fetch_packages().await?;
        let env = self.fetch_environment_all().await?;
        let native_functions = self.native_functions.clone();
        let multithreaded = self.config.is_multithreaded();

        let work = move || execute_run_code(request, packages, env, native_functions);
        if multithreaded {
            tokio::task::spawn_blocking(work)
                .await
                .map_err(|error| CodeModeError::WorkerJoin {
                    reason: error.to_string(),
                })?
        } else {
            work()
        }
    }

    async fn fetch_environment_all(&self) -> CodeModeResult<Vec<EnvironmentVariable>> {
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
    pub fn build(self) -> CodeModeResult<CodeMode> {
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

fn execute_run_code(
    request: RunCode,
    packages: Vec<Package>,
    env: Vec<EnvironmentVariable>,
    native_functions: NativeFunctionRegistry,
) -> CodeModeResult<RunCodeResult> {
    let execution = catch_unwind(AssertUnwindSafe(|| {
        let entered_runtime = tokio::runtime::Handle::try_current().is_err();
        let owned_runtime = if entered_runtime {
            Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|source| CodeModeError::TokioRuntimeInit { source })?,
            )
        } else {
            None
        };
        let _runtime_guard = owned_runtime.as_ref().map(tokio::runtime::Runtime::enter);

        let started_at = Instant::now();
        let loader = Rc::new(CodeModeModuleLoader::new(packages, request.imports));
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![host_extension()],
            module_loader: Some(loader),
            ..RuntimeOptions::default()
        });

        install_host_functions(&mut runtime, env, native_functions)?;

        let function = runtime
            .execute_script(
                "file:///__codemode_exec__.js",
                format!("({})", request.code),
            )
            .map_err(|error| CodeModeError::CompileCode {
                reason: error.to_string(),
            })?;

        let function = {
            deno_core::scope!(scope, runtime);
            let local = v8::Local::new(scope, function);
            if !local.is_function() {
                return Err(CodeModeError::InvalidClosureShape);
            }
            let function = v8::Local::<v8::Function>::try_from(local)
                .map_err(|_| CodeModeError::ClosureNotCallable)?;
            v8::Global::new(scope, function)
        };

        let call = runtime.call(&function);
        let value = deno_core::futures::executor::block_on(
            runtime.with_event_loop_promise(call, PollEventLoopOptions::default()),
        )
        .map_err(|error| CodeModeError::ExecuteCode {
            reason: error.to_string(),
        })?;

        let value = {
            deno_core::scope!(scope, runtime);
            let local = v8::Local::new(scope, value);
            serde_v8::from_v8(scope, local).unwrap_or(serde_json::Value::Null)
        };

        Ok(RunCodeResult {
            value,
            duration: started_at.elapsed(),
        })
    }));

    match execution {
        Ok(result) => result,
        Err(_) => Err(CodeModeError::IsolatePanicked),
    }
}
