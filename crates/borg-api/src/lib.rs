mod controllers;

use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::{get, post, put},
};
use borg_db::BorgDb;
use borg_exec::ExecEngine;
use borg_ltm::MemoryStore;
use borg_ports::{BorgPortsSupervisor, HttpPort, init_http_port};
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use tracing::info;

use crate::controllers::db::DbController;
use crate::controllers::system::SystemController;

#[cfg(test)]
pub(crate) use crate::controllers::system::{HttpPortRequest, validate_port_request};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: BorgDb,
    pub(crate) http_port: HttpPort,
    pub(crate) memory: MemoryStore,
    pub(crate) ports_supervisor: BorgPortsSupervisor,
}

pub struct BorgApiServer {
    bind: String,
    state: AppState,
}

impl BorgApiServer {
    pub fn new(bind: String, db: BorgDb, exec: ExecEngine, memory: MemoryStore) -> Self {
        Self {
            bind,
            state: AppState {
                db: db.clone(),
                http_port: init_http_port(exec.clone()).expect("failed to initialize http port"),
                memory,
                ports_supervisor: BorgPortsSupervisor::new(db, exec.clone()),
            },
        }
    }

    pub async fn run(self) -> Result<()> {
        let ports_supervisor = self.state.ports_supervisor.clone();
        let ports_task = ports_supervisor.clone().start();
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
        ports_supervisor.shutdown().await;

        Ok(())
    }
}

fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(SystemController::ui_dashboard))
        .route("/dashboard", get(SystemController::ui_dashboard))
        .route("/health", get(SystemController::health))
        .route("/ports/http", post(SystemController::ports_http))
        .route("/tasks", get(SystemController::list_tasks))
        .route("/tasks/:id", get(SystemController::get_task))
        .route("/tasks/:id/events", get(SystemController::get_task_events))
        .route("/tasks/:id/output", get(SystemController::get_task_output))
        .route("/memory/search", get(SystemController::memory_search))
        .route(
            "/api/memory/explorer",
            get(SystemController::memory_explorer),
        )
        .route(
            "/memory/entities/:id",
            get(SystemController::get_memory_entity),
        )
        .route(
            "/api/observability/llm-calls",
            get(DbController::list_llm_calls),
        )
        .route("/api/providers", get(DbController::list_providers))
        .route(
            "/api/providers/:provider",
            get(DbController::get_provider)
                .put(DbController::upsert_provider)
                .delete(DbController::delete_provider),
        )
        .route(
            "/api/providers/openai/device-code/start",
            post(DbController::start_openai_device_code),
        )
        .route("/api/policies", get(DbController::list_policies))
        .route(
            "/api/policies/:policy_id",
            get(DbController::get_policy)
                .put(DbController::upsert_policy)
                .delete(DbController::delete_policy),
        )
        .route(
            "/api/policies/:policy_id/uses",
            get(DbController::list_policy_uses),
        )
        .route(
            "/api/policies/:policy_id/uses/:entity_id",
            put(DbController::attach_policy_to_entity)
                .delete(DbController::detach_policy_from_entity),
        )
        .route("/api/agents/specs", get(DbController::list_agent_specs))
        .route(
            "/api/agents/specs/:agent_id",
            get(DbController::get_agent_spec)
                .put(DbController::upsert_agent_spec)
                .delete(DbController::delete_agent_spec),
        )
        .route(
            "/api/users",
            get(DbController::list_users).post(DbController::upsert_user),
        )
        .route(
            "/api/users/:user_key",
            get(DbController::get_user)
                .patch(DbController::patch_user)
                .delete(DbController::delete_user),
        )
        .route(
            "/api/sessions",
            get(DbController::list_sessions).post(DbController::upsert_session),
        )
        .route(
            "/api/sessions/:session_id",
            get(DbController::get_session)
                .patch(DbController::patch_session)
                .delete(DbController::delete_session),
        )
        .route(
            "/api/sessions/:session_id/messages",
            get(DbController::list_session_messages)
                .post(DbController::append_session_message)
                .delete(DbController::clear_session_messages),
        )
        .route(
            "/api/sessions/:session_id/messages/:message_index",
            get(DbController::get_session_message)
                .patch(DbController::patch_session_message)
                .delete(DbController::delete_session_message),
        )
        .route(
            "/api/ports/:port",
            axum::routing::delete(DbController::delete_port),
        )
        .route("/api/ports", get(DbController::list_ports))
        .route(
            "/api/ports/:port/settings",
            get(DbController::list_port_settings),
        )
        .route(
            "/api/ports/:port/settings/:key",
            get(DbController::get_port_setting)
                .put(DbController::upsert_port_setting)
                .delete(DbController::delete_port_setting),
        )
        .route(
            "/api/ports/:port/bindings",
            get(DbController::list_port_bindings),
        )
        .route(
            "/api/ports/:port/bindings/:conversation_key",
            get(DbController::get_port_binding)
                .put(DbController::upsert_port_binding)
                .delete(DbController::delete_port_binding),
        )
        .route(
            "/api/ports/:port/sessions/:session_id/context",
            get(DbController::get_port_session_context)
                .put(DbController::upsert_port_session_context)
                .delete(DbController::delete_port_session_context),
        )
        .route(
            "/api/sessions/:session_id/context",
            get(DbController::get_any_port_session_context),
        )
        .layer(http_trace_layer())
        .layer(cors_layer())
        .with_state(state)
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(
            |origin: &HeaderValue, _request_head| {
                origin
                    .to_str()
                    .ok()
                    .is_some_and(is_allowed_localhost_origin)
            },
        ))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

fn http_trace_layer()
-> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(
            DefaultMakeSpan::new()
                .level(Level::INFO)
                .include_headers(false),
        )
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

fn is_allowed_localhost_origin(origin: &str) -> bool {
    const LOCALHOST_HTTP: &str = "http://localhost";
    const LOCALHOST_HTTPS: &str = "https://localhost";
    const LOOPBACK_HTTP: &str = "http://127.0.0.1";
    const LOOPBACK_HTTPS: &str = "https://127.0.0.1";

    origin == LOCALHOST_HTTP
        || origin == LOCALHOST_HTTPS
        || origin == LOOPBACK_HTTP
        || origin == LOOPBACK_HTTPS
        || origin.starts_with(&format!("{LOCALHOST_HTTP}:"))
        || origin.starts_with(&format!("{LOCALHOST_HTTPS}:"))
        || origin.starts_with(&format!("{LOOPBACK_HTTP}:"))
        || origin.starts_with(&format!("{LOOPBACK_HTTPS}:"))
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, BorgPortsSupervisor, HttpPortRequest, app_router, validate_port_request,
    };
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use borg_core::Uri;
    use borg_db::BorgDb;
    use borg_exec::ExecEngine;
    use borg_ltm::MemoryStore;
    use borg_ports::init_http_port;
    use borg_rt::CodeModeRuntime;
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

        let exec = ExecEngine::new(
            db.clone(),
            memory.clone(),
            CodeModeRuntime::default(),
            Uri::parse("borg:worker:test").expect("worker uri"),
        );
        let http_port = init_http_port(exec.clone()).expect("init http port");
        let state = AppState {
            db: db.clone(),
            http_port,
            memory,
            ports_supervisor: BorgPortsSupervisor::disabled(db, exec),
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
        assert_eq!(parsed.user_key.as_str(), "borg:user:test");
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

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "openai");
        let (status, body) = request_no_body(&app, Method::GET, "/api/providers/openrouter").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["provider"]["provider"], "openrouter");

        let (status, body) = request_no_body(&app, Method::GET, "/api/providers").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["providers"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/providers/openai").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
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
                "model":"gpt-4o-mini",
                "system_prompt":"you are borg",
                "tools":[{"name":"search"}]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/agents/specs/borg:agent:default").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["agent_spec"]["model"], "gpt-4o-mini");

        let (status, body) = request_no_body(&app, Method::GET, "/api/agents/specs").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["agent_specs"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );

        let (status, _) =
            request_no_body(&app, Method::DELETE, "/api/agents/specs/borg:agent:default").await;
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
            "/api/ports/telegram/settings/bot_token",
            json!({"value":"123:abc"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/settings/bot_token").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["value"], "123:abc");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["settings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, body) = request_no_body(&app, Method::GET, "/api/ports?limit=100").await;
        assert_eq!(status, StatusCode::OK);
        let ports = body["ports"].as_array().expect("ports array");
        let telegram = ports
            .iter()
            .find(|port| port["port"] == "telegram")
            .expect("telegram port row");
        assert_eq!(telegram["provider"], "telegram");
        assert!(telegram["enabled"].is_boolean());
        assert!(telegram["active_sessions"].is_number());

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/telegram/settings/bot_token",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(&app, Method::DELETE, "/api/ports/telegram").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn port_bindings_and_context_endpoints_work() {
        let app = test_app("port-bindings-context").await;
        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/telegram/bindings/borg:user:chat1",
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
            "/api/ports/telegram/bindings/borg:user:chat1",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["binding"]["session_id"], "borg:session:s1");

        let (status, body) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/bindings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["bindings"].as_array().is_some_and(|v| !v.is_empty()));

        let (status, _) = request_json(
            &app,
            Method::PUT,
            "/api/ports/telegram/sessions/borg:session:s1/context",
            json!({"ctx":{"chat_id":"123"}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/sessions/borg:session:s1/context",
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
            "/api/ports/telegram/sessions/borg:session:s1/context",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, _) = request_no_body(
            &app,
            Method::DELETE,
            "/api/ports/telegram/bindings/borg:user:chat1",
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
        let (status, _) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/settings/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) =
            request_no_body(&app, Method::GET, "/api/ports/telegram/bindings/not-a-uri").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = request_no_body(
            &app,
            Method::GET,
            "/api/ports/telegram/sessions/not-a-uri/context",
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
