use anyhow::{Result, anyhow};
use deno_core::{JsRuntime, serde_v8, v8};
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};
use tracing::info;

use crate::engine::CodeModeContext;
use crate::types::FfiRegistry;

#[derive(Clone)]
struct FfiBridge {
    ctx: CodeModeContext,
    registry: FfiRegistry,
}

impl FfiBridge {
    fn from_json(registry: FfiRegistry, context_json: Value) -> Result<Self> {
        let ctx = CodeModeContext::from_json(&context_json)?;
        Ok(Self { ctx, registry })
    }

    fn call(&self, op_name: &str, args: Vec<Value>) -> Result<Value> {
        match op_name {
            "context__current" => Ok(self.ctx.to_json()),
            "memory__state_facts" => {
                let args = self.with_default_fact_sources(args)?;
                self.dispatch(op_name, args)
            }
            _ => self.dispatch(op_name, args),
        }
    }

    fn dispatch(&self, op_name: &str, args: Vec<Value>) -> Result<Value> {
        let Some(handler) = self.registry.get(op_name) else {
            return Err(anyhow!("ffi op not found: {}", op_name));
        };
        handler(args)
    }

    fn with_default_fact_sources(&self, mut args: Vec<Value>) -> Result<Vec<Value>> {
        let Some(default_source) = self
            .ctx
            .current_message_id
            .as_ref()
            .map(ToString::to_string)
        else {
            return Ok(args);
        };

        let Some(first_arg) = args.get_mut(0) else {
            return Ok(args);
        };
        let Some(facts) = first_arg.as_array_mut() else {
            return Ok(args);
        };

        for fact in facts.iter_mut() {
            let Some(obj) = fact.as_object_mut() else {
                continue;
            };
            let has_source = obj.get("source").map(|v| !v.is_null()).unwrap_or(false);
            if !has_source {
                obj.insert("source".to_string(), Value::String(default_source.clone()));
            }
        }

        Ok(args)
    }
}

pub(crate) fn install_ffi(
    runtime: &mut JsRuntime,
    registry: FfiRegistry,
    context_json: Value,
) -> Result<()> {
    let scope = &mut runtime.handle_scope();
    let bridge = FfiBridge::from_json(registry, context_json)?;
    scope.set_slot(bridge);
    let context = scope.get_current_context();
    let global = context.global(scope);

    let ffi_fn =
        v8::Function::new(scope, ffi_callback).ok_or_else(|| anyhow!("failed to create ffi"))?;
    let ffi_name = v8::String::new(scope, "ffi").ok_or_else(|| anyhow!("invalid ffi symbol"))?;
    if !global
        .set(scope, ffi_name.into(), ffi_fn.into())
        .unwrap_or(false)
    {
        return Err(anyhow!("failed to install ffi global"));
    }
    Ok(())
}

fn ffi_callback(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let throw = |scope: &mut v8::HandleScope, message: &str| {
        if let Some(msg) = v8::String::new(scope, message) {
            let exception = v8::Exception::error(scope, msg);
            scope.throw_exception(exception);
        }
    };

    let Some(bridge) = scope.get_slot::<FfiBridge>().cloned() else {
        info!(target: "borg_codemode", "ffi failure: bridge not available");
        throw(scope, "ffi bridge not available");
        return;
    };

    if args.length() < 1 || !args.get(0).is_string() {
        info!(target: "borg_codemode", "ffi failure: missing op name");
        throw(scope, "ffi requires op name as first argument");
        return;
    }

    let op_name = args.get(0).to_rust_string_lossy(scope);
    let raw_args: Value = if args.length() > 1 {
        serde_v8::from_v8(scope, args.get(1)).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let call_args = match raw_args {
        Value::Array(values) => values,
        Value::Null => vec![],
        value => vec![value],
    };

    let handler_result = catch_unwind(AssertUnwindSafe(|| bridge.call(&op_name, call_args)));
    match handler_result {
        Ok(Ok(value)) => match serde_v8::to_v8(scope, value) {
            Ok(result) => rv.set(result),
            Err(err) => {
                info!(target: "borg_codemode", error = %err, op_name, "ffi serialization failure");
                throw(scope, &format!("ffi serialization error: {}", err))
            }
        },
        Ok(Err(err)) => {
            info!(target: "borg_codemode", error = %err, op_name, "ffi execution failure");
            throw(scope, &format!("ffi execution error: {}", err))
        }
        Err(_) => {
            info!(target: "borg_codemode", op_name, "ffi execution panic");
            throw(scope, "ffi execution panic")
        }
    }
}
