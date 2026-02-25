use std::time::Instant;

use anyhow::{Result, anyhow};
use borg_core::ExecutionResult;
use deno_core::{JsRuntime, RuntimeOptions, serde_v8, v8};
use serde_json::Value;
use tracing::{debug, info};

#[derive(Default, Clone)]
pub struct RuntimeEngine;

impl RuntimeEngine {
    pub fn execute(&self, code: &str) -> Result<ExecutionResult> {
        info!(target: "borg_rt", "executing JS in deno_core runtime");
        debug!(target: "borg_rt", code, "js payload");

        let start = Instant::now();
        let mut runtime = JsRuntime::new(RuntimeOptions::default());
        let script = code.to_owned();
        let value = runtime
            .execute_script("borg_exec.ts", script)
            .map_err(|err| anyhow!("failed to execute script: {}", err))?;

        let result_json: Value = {
            let scope = &mut runtime.handle_scope();
            let local = v8::Local::new(scope, value);
            serde_v8::from_v8(scope, local).unwrap_or(Value::Null)
        };

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
