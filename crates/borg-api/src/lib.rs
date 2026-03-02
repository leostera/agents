mod controllers;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use borg_db::BorgDb;
use borg_exec::{BorgRuntime, BorgSupervisor};
use borg_memory::MemoryStore;
use borg_ports::BorgPortsSupervisor;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::controllers::routes::app_router;

#[cfg(test)]
pub(crate) use crate::controllers::system::{HttpPortRequest, validate_port_request};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: BorgDb,
    pub(crate) memory: MemoryStore,
}

pub struct BorgApiServer {
    bind: String,
    state: AppState,
    ports_supervisor: BorgPortsSupervisor,
}

impl BorgApiServer {
    pub fn new(bind: String, runtime: Arc<BorgRuntime>, supervisor: BorgSupervisor) -> Self {
        let ports_supervisor =
            BorgPortsSupervisor::new(runtime.clone(), Arc::new(supervisor.clone()));
        Self {
            bind,
            state: AppState {
                db: runtime.db.clone(),
                memory: runtime.memory.clone(),
            },
            ports_supervisor,
        }
    }

    pub async fn run(self) -> Result<()> {
        let ports_supervisor = self.ports_supervisor;
        let ports_task = tokio::spawn(async move {
            if let Err(err) = ports_supervisor.start().await {
                error!(
                    target: "borg_api",
                    error = %err,
                    "ports supervisor stopped unexpectedly"
                );
            }
        });
        let router = app_router(self.state);

        let addr: SocketAddr = self.bind.parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!(target: "borg_api", address = %addr, "http api server listening");

        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed waiting for ctrl-c signal");
            info!(target: "borg_api", "received ctrl-c, shutting down");
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await?;
        ports_task.abort();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, HttpPortRequest, app_router, validate_port_request};
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use borg_db::BorgDb;
    use borg_memory::MemoryStore;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    fn test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let pid = std::process::id();
        std::env::temp_dir().join(format!("borg-api-{name}-{pid}-{nanos}"))
    }

    async fn test_app(name: &str) -> axum::Router {
        let root = test_root(name);
        let db_path = root.join("config.db");
        let memory_path = root.join("ltm");
        let search_path = root.join("search");
        std::fs::create_dir_all(&memory_path).expect("create memory path");
        std::fs::create_dir_all(&search_path).expect("create search path");

        let db = BorgDb::open_local(db_path.to_string_lossy().as_ref())
            .await
            .expect("open local db");
        db.migrate().await.expect("migrate db");

        let memory = MemoryStore::new(&memory_path, &search_path).expect("new memory store");
        memory.migrate().await.expect("migrate memory");

        let state = AppState {
            db: db.clone(),
            memory,
        };
        app_router(state)
    }

    async fn request_json(
        app: &axum::Router,
        method: Method,
        path: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("build request"),
            )
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let parsed = if bytes.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&bytes).expect("json response")
        };
        (status, parsed)
    }

    async fn request_no_body(
        app: &axum::Router,
        method: Method,
        path: &str,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let parsed = if bytes.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&bytes).expect("json response")
        };
        (status, parsed)
    }

    #[test]
    fn validate_port_request_rejects_invalid_uri_fields() {
        let request = HttpPortRequest {
            user_key: "not a uri".to_string(),
            text: "hello".to_string(),
            session_id: Some("bad session".to_string()),
            agent_id: Some("bad agent".to_string()),
            metadata: Some(json!({})),
        };
        assert!(validate_port_request(request).is_err());
    }

    #[test]
    fn validate_port_request_accepts_valid_uri_fields() {
        let request = HttpPortRequest {
            user_key: "borg:user:test".to_string(),
            text: "hello".to_string(),
            session_id: Some("borg:session:123".to_string()),
            agent_id: Some("borg:agent:default".to_string()),
            metadata: Some(json!({"a":"b"})),
        };
        let parsed = validate_port_request(request).unwrap();
        assert_eq!(parsed.user_id.as_str(), "borg:user:test");
        assert_eq!(parsed.session_id.unwrap().as_str(), "borg:session:123");
        assert_eq!(parsed.agent_id.unwrap().as_str(), "borg:agent:default");
    }

    #[tokio::test]
    async fn providers_crud_endpoints_work() {
        let app = test_app("providers").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/providers/openai",
            json!({"api_key":"sk-test"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/providers/openrouter",
            json!({"api_key":"or-test"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/providers/openrouter",
            json!({
                "api_key":"or-test",
                "default_text_model":"openrouter/kimi-k2",
                "default_audio_model":"openai/whisper-1"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "openai");
        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/openrouter").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "openrouter");
        assert_eq!(body["provider"]["base_url"], Value::Null);
        assert_eq!(body["provider"]["default_text_model"], "openrouter/kimi-k2");
        assert_eq!(body["provider"]["default_audio_model"], "openai/whisper-1");

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["providers"].as_array().is_some_and(|v| !v.is_empty()));
        let openrouter = body["providers"]
            .as_array()
            .and_then(|providers| {
                providers
                    .iter()
                    .find(|provider| provider["provider"] == "openrouter")
            })
            .cloned()
            .unwrap_or_else(|| json!({}));
        assert_eq!(openrouter["default_text_model"], "openrouter/kimi-k2");
        assert_eq!(openrouter["default_audio_model"], "openai/whisper-1");

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn local_provider_upsert_requires_base_url() {
        let app = test_app("providers-local-validation").await;
        let (status, _) =
            request_json(&app, Method::PUT, "/api/providers/lmstudio", json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/providers/ollama",
            json!({ "base_url": "http://127.0.0.1:11434" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/ollama").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "ollama");
        assert_eq!(body["provider"]["base_url"], "http://127.0.0.1:11434");
        assert_eq!(body["provider"]["api_key"], "");
    }

    #[tokio::test]
    async fn openai_device_code_start_endpoint_is_wired() {
        let app = test_app("providers-device-code").await;
        let (status, _) = request_no_body(
            &app,
            Method::POST,
            "/api/providers/openai/device-code/start",
        )
        .await;
        assert_ne!(status, StatusCode::NOT_FOUND);
        assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn apps_crud_endpoints_work() {
        let app = test_app("apps").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:movieindex",
            json!({
                "name":"MovieIndex",
                "slug":"movieindex",
                "description":"Search legal torrents",
                "status":"active"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:search",
            json!({
                "name":"searchApis",
                "hint":"Search APIs",
                "mode":"codemode",
                "instructions":"Search APIs by keyword",
                "status":"active"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/apps/borg:app:movieindex/capabilities",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["capabilities"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:search",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["capability"]["name"], "searchApis");

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:search",
            json!({
                "name":"searchApisV2",
                "hint":"Updated hint",
                "mode":"mcp",
                "instructions":"Updated instructions",
                "status":"disabled"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:search",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["capability"]["name"], "searchApisV2");
        assert_eq!(body["capability"]["mode"], "mcp");
        assert_eq!(body["capability"]["status"], "disabled");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/apps/borg:app:movieindex").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["app"]["name"], "MovieIndex");

        let (status, body) = request_no_body(&app, Method::GET, "/api/apps").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["apps"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:search",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/apps/borg:app:movieindex/capabilities",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["capabilities"].as_array().map(|v| v.len()), Some(0));

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/apps/borg:app:movieindex").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn app_capabilities_are_removed_when_app_is_deleted() {
        let app = test_app("apps-capabilities-cascade").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:cascade",
            json!({
                "name":"Cascade App",
                "slug":"cascade-app",
                "description":"cascade test",
                "status":"active"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:cascade/capabilities/borg:capability:one",
            json!({
                "name":"capOne",
                "mode":"codemode"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/apps/borg:app:cascade").await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/apps/borg:app:cascade/capabilities").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cors_allows_localhost_origins() {
        let app = test_app("cors-localhost").await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/providers")
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let allow_origin = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok());
        assert_eq!(allow_origin, Some("http://localhost:5173"));
    }

    #[tokio::test]
    async fn policies_and_policy_uses_crud_endpoints_work() {
        let app = test_app("policies").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:session-read",
            json!({"policy":{"effect":"allow","actions":["session.read"]}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/policies/borg:policy:session-read").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["policy"]["policy"]["effect"], "allow");

        let (status, body) = request_no_body(&app, Method::GET, "/api/policies").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["policies"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:session-read/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/policies/borg:policy:session-read/uses",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["uses"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/policies/borg:policy:session-read/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/policies/borg:policy:session-read",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn agent_specs_crud_endpoints_work() {
        let app = test_app("agent-specs").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/agents/specs/borg:agent:default",
            json!({
                "default_provider_id":"openai",
                "model":"gpt-4o-mini",
                "system_prompt":"you are borg"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["agent_spec"]["model"], "gpt-4o-mini");
        assert_eq!(body["agent_spec"]["enabled"], true);
        assert_eq!(body["agent_spec"]["default_provider_id"], "openai");

        let (status, body) = request_no_body(&app, Method::GET, "/api/agents/specs").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["agent_specs"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/agents/specs/borg:agent:default/enabled",
            json!({ "enabled": false }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["agent_spec"]["enabled"], false);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn actors_crud_endpoints_work() {
        let app = test_app("actors").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/actors/devmode:actor:default",
            json!({
                "system_prompt":"you are actor borg",
                "status":"RUNNING"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/actors/devmode:actor:default").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["actor"]["status"], "RUNNING");
        assert_eq!(body["actor"]["system_prompt"], "you are actor borg");

        let (status, body) = request_no_body(&app, Method::GET, "/api/actors").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["actors"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/actors/devmode:actor:default").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn users_crud_endpoints_work() {
        let app = test_app("users").await;
        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/users",
            json!({"user_key":"borg:user:test","profile":{"name":"Test"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/users/borg:user:test").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["profile"]["name"], "Test");

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/users/borg:user:test",
            json!({"profile":{"name":"Updated"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/users").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["users"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/users/borg:user:test").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_settings_crud_endpoints_work() {
        let app = test_app("port-settings").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram",
            json!({
                "provider":"telegram",
                "enabled": true,
                "allows_guests": true,
                "settings": {
                    "allowed_external_user_ids": ["2654566", "@leostera"]
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram/settings/bot_token",
            json!({"value":"123:abc"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/settings/bot_token",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["value"], "123:abc");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/borg:port:telegram/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["settings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, body) = request_no_body(&app, Method::GET, "/api/ports?limit=100").await;
        assert_eq!(status, StatusCode::OK);
        let ports = body["ports"].as_array().expect("ports array");
        let telegram = ports
            .iter()
            .find(|port| port["port_name"] == "telegram")
            .expect("telegram port row");
        assert_eq!(telegram["provider"], "telegram");
        assert!(telegram["enabled"].is_boolean());
        assert!(telegram["active_sessions"].is_number());

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/borg:port:telegram/settings/bot_token",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/ports/borg:port:telegram").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn discord_port_settings_crud_endpoints_work() {
        let app = test_app("discord-port-settings").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:discord",
            json!({
                "provider":"discord",
                "enabled": true,
                "allows_guests": false,
                "settings": {
                    "bot_token": "discord-token",
                    "allowed_external_user_ids": ["123456789012345678"]
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(&app, Method::GET, "/api/ports?limit=100").await;
        assert_eq!(status, StatusCode::OK);
        let ports = body["ports"].as_array().expect("ports array");
        let discord = ports
            .iter()
            .find(|port| port["port_name"] == "discord")
            .expect("discord port row");
        assert_eq!(discord["provider"], "discord");

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/ports/borg:port:discord").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_bindings_and_context_endpoints_work() {
        let app = test_app("port-bindings-context").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram/bindings/borg:user:chat1",
            json!({
                "session_id":"borg:session:s1",
                "agent_id":"borg:agent:default"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/bindings/borg:user:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["binding"]["session_id"], "borg:session:s1");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/borg:port:telegram/bindings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["bindings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram/sessions/borg:session:s1/context",
            json!({"ctx":{"chat_id":"123"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/sessions/borg:session:s1/context",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ctx"]["chat_id"], "123");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/sessions/borg:session:s1/context").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["port"], "telegram");

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/borg:port:telegram/sessions/borg:session:s1/context",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/borg:port:telegram/bindings/borg:user:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_actor_bindings_endpoints_work() {
        let app = test_app("port-actor-bindings").await;

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/actors/devmode:actor:default",
            json!({
                "name": "Default Actor",
                "system_prompt": "You are the default actor.",
                "status": "RUNNING"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram/actor-bindings/telegram:conversation:chat1",
            json!({
                "actor_id":"devmode:actor:default"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/actor-bindings/telegram:conversation:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["binding"]["actor_id"], "devmode:actor:default");

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/actor-bindings",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["bindings"].as_array().is_some_and(|items| !items.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/borg:port:telegram/actor-bindings/telegram:conversation:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn sessions_and_messages_crud_endpoints_work() {
        let app = test_app("sessions").await;
        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/sessions",
            json!({
                "session_id":"borg:session:test",
                "users":["borg:user:test"],
                "port":"borg:port:http"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/sessions/borg:session:test").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["port"], "borg:port:http");
        assert_eq!(body["session"]["users"][0], "borg:user:test");

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/sessions/borg:session:test",
            json!({"users":["borg:user:test", "borg:user:other"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _) = request_json(
            &app,
            Method::POST,
            "/api/sessions/borg:session:test/messages",
            json!({"payload":{"role":"user","content":"hello"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:test/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["message"]["message_index"], 0);

        let (status, _) = request_json(
            &app,
            Method::PATCH,
            "/api/sessions/borg:session:test/messages/0",
            json!({"payload":{"role":"user","content":"updated"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:test/messages?from=0&limit=10",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/sessions/borg:session:test/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/sessions/borg:session:test/messages",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/sessions/borg:session:test").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn providers_negative_paths() {
        let app = test_app("providers-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/providers/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/providers/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_json(&app, Method::PUT, "/api/providers/openai", json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_json(&app, Method::PUT, "/api/providers/not-real", json!({})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn apps_negative_paths() {
        let app = test_app("apps-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/apps/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(&app, Method::GET, "/api/apps/borg:app:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/apps/borg:app:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/apps/borg:app:missing/capabilities").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:missing/capabilities/borg:capability:search",
            json!({
                "name":"searchApis",
                "mode":"codemode"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:movieindex/capabilities/not-a-uri",
            json!({
                "name":"searchApis",
                "mode":"codemode"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/apps/borg:app:movieindex/capabilities/borg:capability:empty-name",
            json!({
                "name":"  ",
                "mode":"codemode"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn policies_negative_paths() {
        let app = test_app("policies-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/policies/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/policies/borg:policy:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/borg:policy:missing/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::PUT,
            "/api/policies/not-a-uri/uses/borg:agent:default",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn agents_negative_paths() {
        let app = test_app("agents-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/agents/specs/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/agents/specs/borg:agent:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn actors_negative_paths() {
        let app = test_app("actors-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/actors/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/actors/devmode:actor:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn users_negative_paths() {
        let app = test_app("users-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/users/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/users/borg:user:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sessions_negative_paths() {
        let app = test_app("sessions-negative").await;
        let (status, _) = request_no_body(&app, Method::GET, "/api/sessions/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/sessions/borg:session:missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:missing/messages/0",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ports_negative_paths() {
        let app = test_app("ports-negative").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:telegram",
            json!({
                "provider":"telegram",
                "enabled": true,
                "allows_guests": false,
                "settings": {
                    "allowed_external_user_ids": ["not-valid-user-format"]
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/borg:port:discord",
            json!({
                "provider":"discord",
                "enabled": true,
                "allows_guests": false,
                "settings": {
                    "allowed_external_user_ids": ["@not-a-discord-id"]
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/settings/missing",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/bindings/not-a-uri",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/borg:port:telegram/sessions/not-a-uri/context",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/sessions/borg:session:missing/context",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
