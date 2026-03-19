use worker::*;

#[event(fetch)]
pub async fn fetch(_req: Request, _env: Env, _ctx: worker::Context) -> Result<Response> {
    Response::ok("hello from cloudflare-worker-agent")
}
