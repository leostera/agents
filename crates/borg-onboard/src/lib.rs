use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{Router, response::Html, routing::get};
use tokio::net::TcpListener;
use tracing::info;

#[derive(Debug, Clone)]
pub struct OnboardServer {
    addr: SocketAddr,
}

impl OnboardServer {
    pub fn new(port: u16) -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
        }
    }

    pub async fn run(self) -> Result<()> {
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/onboard", get(onboard_page));

        let listener = TcpListener::bind(self.addr)
            .await
            .with_context(|| format!("failed to bind onboarding server to {}", self.addr))?;

        info!(target: "borg_onboard", address = %self.addr, "onboarding web server listening");
        axum::serve(listener, app)
            .await
            .context("onboarding server failure")?;

        Ok(())
    }
}

async fn onboard_page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
  <head>
    <meta charset='utf-8'/>
    <meta name='viewport' content='width=device-width, initial-scale=1'/>
    <title>Borg Onboarding</title>
    <style>
      body { font-family: ui-monospace, Menlo, monospace; margin: 0; padding: 40px; background: #0f172a; color: #e2e8f0; }
      h1 { margin: 0 0 10px; }
      p { color: #93c5fd; max-width: 680px; }
    </style>
  </head>
  <body>
    <h1>Borg Onboarding</h1>
    <p>Onboarding server is running. Next onboarding steps will be implemented here.</p>
  </body>
</html>"#,
    )
}
