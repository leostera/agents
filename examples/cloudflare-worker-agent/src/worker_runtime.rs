use agents::{
    Agent, LlmRunner,
    provider::cloudflare::workers_ai::{WorkersAI, WorkersAIConfig},
};
use std::sync::Arc;
use worker::*;

use crate::echo::{CloudEchoAgent, CloudEchoRequest, CloudEchoResponse};

const WORKERS_AI_BINDING: &str = "AI";
const WORKERS_AI_MODEL_ENV: &str = "WORKERS_AI_MODEL";
const DEFAULT_WORKERS_AI_MODEL: &str = "@cf/meta/llama-3.1-8b-instruct";
const JSON_CONTENT_TYPE: &str = "application/json";

macro_rules! worker_info {
    ($($arg:tt)*) => {
        worker::console_log!($($arg)*);
    };
}

macro_rules! worker_error {
    ($($arg:tt)*) => {
        worker::console_error!($($arg)*);
    };
}

fn missing_env_error(name: &str) -> Error {
    Error::RustError(format!("missing required worker binding `{name}`"))
}

fn method_not_allowed() -> Result<Response> {
    let mut response = Response::error("method not allowed", 405)?;
    response.headers_mut().set("allow", "POST")?;
    Ok(response)
}

fn bad_request(message: &str) -> Result<Response> {
    Response::error(message, 400)
}

fn not_found() -> Result<Response> {
    Response::error("not found", 404)
}

async fn build_llm_runner(env: &Env) -> Result<Arc<LlmRunner>> {
    worker_info!("building llm runner from Workers AI binding");
    let ai = env
        .ai(WORKERS_AI_BINDING)
        .map_err(|_| missing_env_error(WORKERS_AI_BINDING))?;
    let model = env
        .var(WORKERS_AI_MODEL_ENV)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| DEFAULT_WORKERS_AI_MODEL.to_string());
    worker_info!(
        "resolved Workers AI config model={} binding={}",
        model,
        WORKERS_AI_BINDING
    );

    let config = WorkersAIConfig::from_binding(model).with_binding(WORKERS_AI_BINDING);
    let provider = WorkersAI::new(config).with_binding(ai);
    worker_info!("Workers AI provider constructed");
    Ok(Arc::new(
        LlmRunner::builder().add_provider(provider).build(),
    ))
}

#[event(fetch)]
pub async fn fetch(mut req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    let path = req.path();

    if req.method() == Method::Get && path == "/favicon.ico" {
        return Response::empty();
    }

    if path != "/" {
        return not_found();
    }

    if req.method() == Method::Get {
        worker_info!(
            "worker health request method={:?} path={}",
            req.method(),
            path
        );
        return Response::ok(
            "cloudflare-worker-agent is running; POST JSON to / with {\"text\":\"...\"}",
        );
    }

    if req.method() != Method::Post {
        return method_not_allowed();
    }

    let has_json_body = req
        .headers()
        .get("content-type")?
        .map(|value| value.to_ascii_lowercase().starts_with(JSON_CONTENT_TYPE))
        .unwrap_or(false);

    if !has_json_body {
        return bad_request("expected application/json request body");
    }

    let input: CloudEchoRequest = req.json().await?;
    worker_info!("received echo request path={} input={}", path, input.text);
    worker_info!("starting llm runner setup");
    let llm_runner = build_llm_runner(&env).await?;
    worker_info!("llm runner ready");
    worker_info!("starting agent construction");
    let mut agent = CloudEchoAgent::new(llm_runner).await.map_err(|error| {
        worker_error!("failed to initialize agent error={}", error);
        Error::RustError(error.to_string())
    })?;
    worker_info!("agent ready");
    worker_info!("calling agent");
    let reply = agent.call(input).await.map_err(|error| {
        worker_error!("agent call failed error={}", error);
        Error::RustError(error.to_string())
    })?;
    worker_info!("echo request completed reply={}", reply);
    Response::from_json(&CloudEchoResponse { text: reply })
}
