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

    pub fn execute(&self, code: &str) -> Result<ExecutionResult> {
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

        let value = runtime
            .execute_script("borg_exec.js", code.to_owned())
            .map_err(|err| anyhow!("failed to execute script: {}", err))?;

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
