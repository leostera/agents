use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use anyhow::{Result, anyhow};
use borg_core::ExecutionResult;
use borg_core::Uri;
use deno_core::{JsRuntime, RuntimeOptions, serde_v8, v8};
use serde_json::Value;
use tracing::{debug, info, trace};

use crate::ffi::install_ffi;
use crate::module_loader::CodeModeModuleLoader;
use crate::ops::default_ffi_handlers;
use crate::sdk::{ApiCapability, search_capabilities};
use crate::types::{FfiHandler, FfiResult};

const SDK_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/borg_agent_sdk.bundle.js"));
static RUNTIME_EXECUTION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const PRELUDE_SCRIPT_ORIGIN: &str = "file:///__borg_prelude__.js";
const SDK_SCRIPT_ORIGIN: &str = "file:///__borg_agent_sdk__.js";
const EXEC_SCRIPT_ORIGIN: &str = "file:///__borg_exec_fn__.js";

#[derive(Debug, Clone, Default)]
pub struct CodeModeContext {
    pub current_port_id: Option<Uri>,
    pub current_message_id: Option<Uri>,
    pub current_actor_id: Option<Uri>,
    pub current_user_id: Option<Uri>,
    pub env: HashMap<String, String>,
}

impl CodeModeContext {
    pub fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(value) = &self.current_port_id {
            obj.insert(
                "current_port_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_message_id {
            obj.insert(
                "current_message_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_actor_id {
            obj.insert(
                "current_actor_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_user_id {
            obj.insert(
                "current_user_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if !self.env.is_empty() {
            let env_obj = self
                .env
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect();
            obj.insert("env".to_string(), Value::Object(env_obj));
        }
        let env_keys = self.available_env_keys();
        if !env_keys.is_empty() {
            obj.insert(
                "available_env_keys".to_string(),
                Value::Array(env_keys.into_iter().map(Value::String).collect()),
            );
        }
        Value::Object(obj)
    }

    pub fn to_public_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(value) = &self.current_port_id {
            obj.insert(
                "current_port_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_message_id {
            obj.insert(
                "current_message_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_actor_id {
            obj.insert(
                "current_actor_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = &self.current_user_id {
            obj.insert(
                "current_user_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        let env_keys = self.available_env_keys();
        if !env_keys.is_empty() {
            obj.insert(
                "available_env_keys".to_string(),
                Value::Array(env_keys.into_iter().map(Value::String).collect()),
            );
        }
        Value::Object(obj)
    }

    pub fn from_json(value: &Value) -> Result<Self> {
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow!("context must be an object"))?;
        Ok(Self {
            current_port_id: parse_uri_field(obj, "current_port_id")?,
            current_message_id: parse_uri_field(obj, "current_message_id")?,
            current_actor_id: parse_uri_field(obj, "current_actor_id")?,
            current_user_id: parse_uri_field(obj, "current_user_id")?,
            env: parse_env_field(obj)?,
        })
    }

    pub fn available_env_keys(&self) -> Vec<String> {
        let mut keys = self.env.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    }
}

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

    pub fn execute(&self, code: &str, context: CodeModeContext) -> Result<ExecutionResult<Value>> {
        let _runtime_lock = runtime_execution_lock()
            .lock()
            .map_err(|_| anyhow!("code-mode runtime execution lock poisoned"))?;

        let execution = catch_unwind(AssertUnwindSafe(|| {
            info!(target: "borg_codemode", "executing JS in deno_core runtime");
            debug!(target: "borg_codemode", code, "js payload");

            let start = Instant::now();
            let mut runtime = JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(CodeModeModuleLoader::new())),
                ..RuntimeOptions::default()
            });
            install_ffi(
                &mut runtime,
                Arc::new(self.ffi_handlers.clone()),
                context.to_json(),
            )?;

            if !self.prelude.trim().is_empty() {
                runtime
                    .execute_script(PRELUDE_SCRIPT_ORIGIN, self.prelude.clone())
                    .map_err(|err| {
                        info!(target: "borg_codemode", error = %err, "code-mode prelude execution failure");
                        anyhow!(normalize_runtime_error(format!(
                            "failed to execute prelude: {}",
                            err
                        )))
                    })?;
            }
            runtime
                .execute_script(SDK_SCRIPT_ORIGIN, self.sdk_bundle.clone())
                .map_err(|err| {
                    info!(target: "borg_codemode", error = %err, "code-mode sdk execution failure");
                    anyhow!(normalize_runtime_error(format!(
                        "failed to execute sdk bundle: {}",
                        err
                    )))
                })?;

            let function = runtime
                .execute_script(EXEC_SCRIPT_ORIGIN, format!("({})", code))
                .map_err(|err| {
                    info!(target: "borg_codemode", error = %err, "code-mode function compile failure");
                    anyhow!(normalize_runtime_error(format!(
                        "failed to compile code-mode function: {}",
                        err
                    )))
                })?;
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
                .map_err(|err| {
                info!(target: "borg_codemode", error = %err, "code-mode function execution failure");
                anyhow!(normalize_runtime_error(format!(
                    "failed to execute code-mode function: {}",
                    err
                )))
            })?;

            let result: Value = {
                let scope = &mut runtime.handle_scope();
                let local = v8::Local::new(scope, value);
                serde_v8::from_v8(scope, local).unwrap_or(Value::Null)
            };

            trace!(target: "borg_codemode", result = ?result, "js execution result");
            let duration = start.elapsed();
            let duration_ms = duration.as_millis();
            info!(target: "borg_codemode", duration_ms, "JS execution finished");

            Ok(ExecutionResult {
                stdout: String::new(),
                stderr: String::new(),
                result,
                duration,
            })
        }));

        match execution {
            Ok(result) => result,
            Err(payload) => {
                let panic_text = panic_payload_to_string(payload);
                info!(
                    target: "borg_codemode",
                    panic = panic_text.as_str(),
                    "code-mode runtime panic caught"
                );
                Err(anyhow!(
                    "code-mode runtime panicked during execution: {}",
                    panic_text
                ))
            }
        }
    }
}

fn parse_uri_field(obj: &serde_json::Map<String, Value>, field: &str) -> Result<Option<Uri>> {
    let Some(raw) = obj.get(field) else {
        return Ok(None);
    };
    let text = raw
        .as_str()
        .ok_or_else(|| anyhow!("context field `{field}` must be a string"))?;
    Ok(Some(Uri::parse(text)?))
}

fn parse_env_field(obj: &serde_json::Map<String, Value>) -> Result<HashMap<String, String>> {
    let Some(raw) = obj.get("env") else {
        return Ok(HashMap::new());
    };
    let env_obj = raw
        .as_object()
        .ok_or_else(|| anyhow!("context field `env` must be an object"))?;
    let mut env = HashMap::with_capacity(env_obj.len());
    for (key, value) in env_obj {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("context field `env.{key}` must be a string"))?;
        env.insert(key.clone(), value.to_string());
    }
    Ok(env)
}

fn runtime_execution_lock() -> &'static Mutex<()> {
    RUNTIME_EXECUTION_LOCK.get_or_init(|| Mutex::new(()))
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

fn normalize_runtime_error(message: String) -> String {
    if message.contains("BorgOs") || message.contains("borgos") {
        return format!(
            "{}. Hint: use `Borg.OS.ls(...)` (the `BorgOs` symbol is invalid).",
            message
        );
    }
    message
}
