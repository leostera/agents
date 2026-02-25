use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use reqwest::Method;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};

use crate::types::{FfiHandler, FfiResult};

pub(crate) fn default_ffi_handlers() -> HashMap<String, FfiHandler> {
    let mut ffi_handlers: HashMap<String, FfiHandler> = HashMap::new();
    ffi_handlers.insert("os__ls".to_string(), Arc::new(default_os_ls));
    ffi_handlers.insert("net__fetch".to_string(), Arc::new(default_net_fetch));
    ffi_handlers
}

fn default_os_ls(args: Vec<Value>) -> FfiResult {
    let mut cmd = Command::new("ls");
    for arg in args {
        let value = arg
            .as_str()
            .ok_or_else(|| anyhow!("os__ls expects string arguments"))?;
        cmd.arg(value);
    }

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "os__ls failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    Ok(json!({ "entries": entries }))
}

fn default_net_fetch(_args: Vec<Value>) -> FfiResult {
    let args = _args;
    let url = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("net__fetch expects first argument to be url string"))?;
    let init = args.get(1).and_then(Value::as_object);

    let method = init
        .and_then(|obj| obj.get("method"))
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|err| anyhow!("net__fetch invalid HTTP method `{}`: {}", method, err))?;

    let headers = parse_headers(init.and_then(|obj| obj.get("headers")))?;
    let body = parse_body(init.and_then(|obj| obj.get("body")))?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| anyhow!("net__fetch failed to create client: {}", err))?;
    let mut request = client.request(method, url).headers(headers);
    if let Some(body) = body {
        request = request.body(body);
    }

    let response = request
        .send()
        .map_err(|err| anyhow!("net__fetch request failed: {}", err))?;

    let status = response.status();
    let status_code = status.as_u16();
    let ok = status.is_success();
    let status_text = status.canonical_reason().unwrap_or("").to_string();
    let final_url = response.url().to_string();

    let mut response_headers = serde_json::Map::new();
    for (name, value) in response.headers() {
        response_headers.insert(
            name.as_str().to_string(),
            Value::String(value.to_str().unwrap_or("").to_string()),
        );
    }

    let body = response
        .text()
        .map_err(|err| anyhow!("net__fetch failed to read response body: {}", err))?;
    let body_json = serde_json::from_str::<Value>(&body).ok();

    Ok(json!({
        "ok": ok,
        "status": status_code,
        "status_text": status_text,
        "url": final_url,
        "headers": response_headers,
        "body": body,
        "json": body_json
    }))
}

fn parse_headers(value: Option<&Value>) -> Result<HeaderMap, anyhow::Error> {
    let mut headers = HeaderMap::new();
    let Some(value) = value else {
        return Ok(headers);
    };

    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("net__fetch init.headers must be an object"))?;
    for (key, value) in obj {
        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|err| anyhow!("invalid header name `{}`: {}", key, err))?;
        let header_value_raw = value
            .as_str()
            .ok_or_else(|| anyhow!("header `{}` must have string value", key))?;
        let header_value = HeaderValue::from_str(header_value_raw)
            .map_err(|err| anyhow!("invalid header value for `{}`: {}", key, err))?;
        headers.insert(header_name, header_value);
    }
    Ok(headers)
}

fn parse_body(value: Option<&Value>) -> Result<Option<String>, anyhow::Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        other => Ok(Some(other.to_string())),
    }
}
