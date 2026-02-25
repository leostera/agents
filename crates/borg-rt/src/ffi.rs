use anyhow::{Result, anyhow};
use deno_core::{JsRuntime, serde_v8, v8};
use serde_json::Value;

use crate::types::FfiRegistry;

pub(crate) fn install_ffi(runtime: &mut JsRuntime, registry: FfiRegistry) -> Result<()> {
    let scope = &mut runtime.handle_scope();
    scope.set_slot(registry);
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

    let Some(registry) = scope.get_slot::<FfiRegistry>().cloned() else {
        throw(scope, "ffi registry not available");
        return;
    };

    if args.length() < 1 || !args.get(0).is_string() {
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

    let Some(handler) = registry.get(&op_name) else {
        throw(scope, &format!("ffi op not found: {}", op_name));
        return;
    };

    match handler(call_args) {
        Ok(value) => match serde_v8::to_v8(scope, value) {
            Ok(result) => rv.set(result),
            Err(err) => throw(scope, &format!("ffi serialization error: {}", err)),
        },
        Err(err) => throw(scope, &format!("ffi execution error: {}", err)),
    }
}
