use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use borg_core::ExecutionResult;
use deno_core::{JsRuntime, RuntimeOptions, serde_v8, v8};
use serde_json::Value;
use tracing::{debug, info, trace};

use crate::ffi::install_ffi;
use crate::ops::default_ffi_handlers;
use crate::sdk::{ApiCapability, search_capabilities};
use crate::types::{FfiHandler, FfiResult};

const SDK_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/borg_agent_sdk.bundle.js"));

#[derive(Clone)]
pub struct CodeModeRuntime {
    prelude: String,
    sdk_bundle: String,
    ffi_handlers: HashMap<String, FfiHandler>,
}

impl Default for CodeModeRuntime {
    fn default() -> Self {
        Self {
            prelude: String::new(),
            sdk_bundle: SDK_BUNDLE.to_string(),
            ffi_handlers: default_ffi_handlers(),
        }
    }
}

impl CodeModeRuntime {
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.prelude = prelude.into();
        self
    }

    pub fn with_sdk_bundle(mut self, sdk_bundle: impl Into<String>) -> Self {
        self.sdk_bundle = sdk_bundle.into();
        self
    }

    pub fn with_ffi_handler(
        mut self,
        op_name: impl Into<String>,
        handler: impl Fn(Vec<Value>) -> FfiResult + Send + Sync + 'static,
    ) -> Self {
        self.ffi_handlers.insert(op_name.into(), Arc::new(handler));
        self
    }

    pub fn search(&self, query: &str) -> Vec<ApiCapability> {
        search_capabilities(query)
    }

    pub fn execute(&self, code: &str) -> Result<ExecutionResult> {
        validate_code_mode_shape(code)?;

        info!(target: "borg_rt", "executing JS in deno_core runtime");
        debug!(target: "borg_rt", code, "js payload");

        let start = Instant::now();
        let mut runtime = JsRuntime::new(RuntimeOptions::default());
        install_ffi(&mut runtime, Arc::new(self.ffi_handlers.clone()))?;

        if !self.prelude.trim().is_empty() {
            runtime
                .execute_script("borg_prelude.js", self.prelude.clone())
                .map_err(|err| anyhow!("failed to execute prelude: {}", err))?;
        }
        runtime
            .execute_script("borg_agent_sdk.js", self.sdk_bundle.clone())
            .map_err(|err| anyhow!("failed to execute sdk bundle: {}", err))?;

        let function = runtime
            .execute_script("borg_exec_fn.js", format!("({})", code))
            .map_err(|err| anyhow!("failed to compile code-mode function: {}", err))?;
        let function = {
            let scope = &mut runtime.handle_scope();
            let local = v8::Local::new(scope, function);
            if !local.is_function() {
                return Err(anyhow!(
                    "code-mode execute expects an async zero-arg function expression"
                ));
            }
            let local_fn = v8::Local::<v8::Function>::try_from(local)
                .map_err(|_| anyhow!("code-mode payload did not resolve to a function"))?;
            v8::Global::new(scope, local_fn)
        };

        #[allow(deprecated)]
        let value = deno_core::futures::executor::block_on(runtime.call_and_await(&function))
            .map_err(|err| anyhow!("failed to execute code-mode function: {}", err))?;

        let result_json: Value = {
            let scope = &mut runtime.handle_scope();
            let local = v8::Local::new(scope, value);
            serde_v8::from_v8(scope, local).unwrap_or(Value::Null)
        };

        trace!(target: "borg_rt", result = ?result_json, "js execution result");
        let duration_ms = start.elapsed().as_millis();
        info!(target: "borg_rt", duration_ms, "JS execution finished");

        Ok(ExecutionResult {
            stdout: String::new(),
            stderr: String::new(),
            result_json,
            duration_ms,
        })
    }
}

fn validate_code_mode_shape(code: &str) -> Result<()> {
    let trimmed = code.trim().trim_end_matches(';').trim();
    if !trimmed.starts_with("async") {
        return Err(anyhow!(
            "execute code must start with `async () => {{ ... }}`"
        ));
    }

    let Some((lhs, rhs)) = trimmed.split_once("=>") else {
        return Err(anyhow!(
            "execute code must use an async arrow function (`async () => {{ ... }}`)"
        ));
    };

    let lhs = lhs.trim();
    if lhs != "async ()" {
        return Err(anyhow!(
            "execute code must have zero arguments (`async () => {{ ... }}`)"
        ));
    }

    let rhs = rhs.trim();
    if !rhs.starts_with('{') || !rhs.ends_with('}') {
        return Err(anyhow!(
            "execute code body must be a block (`async () => {{ ... }}`)"
        ));
    }

    let body = rhs.trim_start_matches('{').trim_end_matches('}').trim();
    if !body.contains("return") {
        return Err(anyhow!(
            "execute code body must include an explicit `return` statement"
        ));
    }

    Ok(())
}
