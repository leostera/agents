use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use deno_core::{JsRuntime, OpState, op2};
use deno_error::JsErrorBox;
use reqwest::Method;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::json;

use crate::native::NativeFunctionRegistry;
use crate::providers::EnvironmentVariable;

#[derive(Clone)]
struct HostBridge {
    env: BTreeMap<String, String>,
    native_functions: NativeFunctionRegistry,
}

#[derive(Deserialize)]
struct FetchInit {
    method: Option<String>,
    headers: Option<serde_json::Value>,
    body: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct FetchRequest {
    url: String,
    init: Option<FetchInit>,
}

#[op2]
#[serde]
fn op_env_get(
    state: &mut OpState,
    #[string] key: String,
    #[serde] fallback: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsErrorBox> {
    let bridge = state.borrow::<HostBridge>();
    Ok(bridge
        .env
        .get(key.trim())
        .map(|value| serde_json::Value::String(value.clone()))
        .or(fallback)
        .unwrap_or(serde_json::Value::Null))
}

#[op2]
#[serde]
fn op_env_keys(state: &mut OpState) -> Result<serde_json::Value, JsErrorBox> {
    let bridge = state.borrow::<HostBridge>();
    Ok(serde_json::Value::Array(
        bridge
            .env
            .keys()
            .cloned()
            .map(serde_json::Value::String)
            .collect(),
    ))
}

#[op2]
#[serde]
fn op_fetch(#[serde] request: FetchRequest) -> Result<serde_json::Value, JsErrorBox> {
    let join = std::thread::spawn(move || fetch(request));
    match join.join() {
        Ok(result) => result.map_err(js_error),
        Err(_) => Err(JsErrorBox::generic("fetch worker panicked")),
    }
}

#[op2]
#[serde]
async fn op_native_call(
    op_state: Rc<RefCell<OpState>>,
    #[string] name: String,
    #[serde] args: serde_json::Value,
) -> Result<serde_json::Value, JsErrorBox> {
    let registry = {
        let op_state = op_state.borrow();
        op_state.borrow::<HostBridge>().native_functions.clone()
    };

    registry.call(&name, args).await.map_err(js_error)
}

deno_core::extension!(
    agents_codemode_host,
    ops = [op_env_get, op_env_keys, op_fetch, op_native_call],
);

pub(crate) fn extension() -> deno_core::Extension {
    agents_codemode_host::init()
}

pub(crate) fn install_host_functions(
    runtime: &mut JsRuntime,
    env: Vec<EnvironmentVariable>,
    native_functions: NativeFunctionRegistry,
) -> Result<()> {
    let env = env
        .into_iter()
        .map(|variable| (variable.name, variable.value))
        .collect::<BTreeMap<_, _>>();

    runtime.op_state().borrow_mut().put(HostBridge {
        env,
        native_functions: native_functions.clone(),
    });

    let bootstrap = bootstrap_script(native_functions.names())?;
    runtime
        .execute_script("file:///__agents_codemode_bootstrap__.js", bootstrap)
        .map_err(|error| anyhow!("failed to install codemode globals: {error}"))?;

    Ok(())
}

fn bootstrap_script(native_names: Vec<String>) -> Result<String> {
    let native_names_json = serde_json::to_string(&native_names)?;
    Ok(format!(
        r#"
const __nativeNames = {native_names_json};
const {{
  op_env_get,
  op_env_keys,
  op_fetch,
  op_native_call,
}} = Deno.core.ops;

globalThis.fetch = (url, init) => Promise.resolve(op_fetch({{ url, init: init ?? null }}));
globalThis.env = {{
  get(key, fallback = null) {{
    return Promise.resolve(op_env_get(key, fallback));
  }},
  keys() {{
    return Promise.resolve(op_env_keys());
  }},
}};

globalThis.native = Object.create(null);
for (const name of __nativeNames) {{
  globalThis.native[name] = (...args) => op_native_call(name, args);
}}
"#,
        native_names_json = native_names_json,
    ))
}

fn js_error(error: anyhow::Error) -> JsErrorBox {
    JsErrorBox::generic(error.to_string())
}

fn fetch(request: FetchRequest) -> Result<serde_json::Value> {
    let init = request.init.as_ref();

    let method = init
        .and_then(|value| value.method.as_deref())
        .unwrap_or("GET");
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|error| anyhow!("invalid fetch method `{method}`: {error}"))?;

    let headers = parse_headers(init.and_then(|value| value.headers.as_ref()))?;
    let body = parse_body(init.and_then(|value| value.body.as_ref()))?;

    let client = Client::builder().build()?;
    let mut request_builder = client.request(method, &request.url).headers(headers);
    if let Some(body) = body {
        request_builder = request_builder.body(body);
    }

    let response = request_builder.send()?;
    let status = response.status();
    let status_code = status.as_u16();
    let ok = status.is_success();
    let status_text = status.canonical_reason().unwrap_or_default().to_string();
    let final_url = response.url().to_string();

    let mut response_headers = serde_json::Map::new();
    for (name, value) in response.headers() {
        response_headers.insert(
            name.as_str().to_string(),
            serde_json::Value::String(value.to_str().unwrap_or_default().to_string()),
        );
    }

    let body = response.text()?;
    let body_json = serde_json::from_str::<serde_json::Value>(&body).ok();

    Ok(json!({
        "ok": ok,
        "status": status_code,
        "status_text": status_text,
        "url": final_url,
        "headers": response_headers,
        "body": body,
        "json": body_json,
    }))
}

fn parse_headers(value: Option<&serde_json::Value>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let Some(value) = value else {
        return Ok(headers);
    };

    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("fetch init.headers must be an object"))?;
    for (key, value) in obj {
        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|error| anyhow!("invalid header name `{key}`: {error}"))?;
        let header_value = value
            .as_str()
            .ok_or_else(|| anyhow!("header `{key}` must be a string"))?;
        let header_value = HeaderValue::from_str(header_value)
            .map_err(|error| anyhow!("invalid value for header `{key}`: {error}"))?;
        headers.insert(header_name, header_value);
    }
    Ok(headers)
}

fn parse_body(value: Option<&serde_json::Value>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.clone())),
        other => Ok(Some(other.to_string())),
    }
}
