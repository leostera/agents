use js_sys::{Function, JSON, Promise, Reflect};
use serde_json::Value;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use worker::Ai;

use super::RunResult;
use crate::llm::error::{Error, LlmResult};

pub(super) async fn execute_run_request(
    binding: Option<&Ai>,
    model: &str,
    body: Value,
    binding_name: &str,
) -> LlmResult<RunResult> {
    let binding = binding.ok_or_else(|| Error::InvalidRequest {
        reason: format!(
            "Workers AI binding transport requires binding `{binding_name}` to be attached via WorkersAI::with_binding"
        ),
    })?;

    let input = JSON::parse(&body.to_string()).map_err(|error| Error::InvalidRequest {
        reason: format!("failed to serialize Workers AI binding request: {error:?}"),
    })?;
    let run = Reflect::get(binding.as_ref(), &JsValue::from_str("run"))
        .map_err(js_binding_error)?
        .dyn_into::<Function>()
        .map_err(js_binding_error)?;
    let promise = run
        .call2(binding.as_ref(), &JsValue::from_str(model), &input)
        .map_err(js_binding_error)?
        .dyn_into::<Promise>()
        .map_err(js_binding_error)?;
    let output = JsFuture::from(promise)
        .await
        .map_err(|error| Error::Provider {
            provider: "workers_ai".to_string(),
            status: 500,
            message: format!("{error:?}"),
        })?;

    from_value(output).map_err(|error| Error::InvalidResponse {
        reason: format!("failed to decode Workers AI binding response: {error}"),
    })
}

fn js_binding_error(error: JsValue) -> Error {
    Error::Provider {
        provider: "workers_ai".to_string(),
        status: 500,
        message: format!("{error:?}"),
    }
}
