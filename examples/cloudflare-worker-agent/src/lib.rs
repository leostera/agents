#[cfg(not(target_arch = "wasm32"))]
evals::setup!();

#[cfg(not(target_arch = "wasm32"))]
pub mod echo;

#[cfg(target_arch = "wasm32")]
use worker::*;

#[event(fetch)]
#[cfg(target_arch = "wasm32")]
pub async fn fetch(_req: Request, _env: Env, _ctx: worker::Context) -> Result<Response> {
    Response::ok("hello from cloudflare-worker-agent")
}
