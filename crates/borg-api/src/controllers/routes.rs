use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::{get, post, put},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::db::DbController;
use super::system::SystemController;
use crate::AppState;

pub(crate) fn app_router(state: AppState) -> Router {
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
            "/api/observability/tool-calls",
            get(DbController::list_tool_calls),
        )
        .route(
            "/api/observability/tool-calls/:call_id",
            get(DbController::get_tool_call),
        )
        .route(
            "/api/observability/llm-calls",
            get(DbController::list_llm_calls),
        )
        .route(
            "/api/observability/llm-calls/:call_id",
            get(DbController::get_llm_call),
        )
        .route("/api/providers", get(DbController::list_providers))
        .route(
            "/api/providers/:provider",
            get(DbController::get_provider)
                .put(DbController::upsert_provider)
                .delete(DbController::delete_provider),
        )
        .route(
            "/api/providers/:provider/models",
            get(DbController::list_provider_models),
        )
        .route("/api/apps", get(DbController::list_apps))
        .route(
            "/api/apps/:app_id",
            get(DbController::get_app)
                .put(DbController::upsert_app)
                .delete(DbController::delete_app),
        )
        .route(
            "/api/apps/:app_id/capabilities",
            get(DbController::list_app_capabilities),
        )
        .route(
            "/api/apps/:app_id/capabilities/:capability_id",
            get(DbController::get_app_capability)
                .put(DbController::upsert_app_capability)
                .delete(DbController::delete_app_capability),
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
            "/api/agents/specs/:agent_id/enabled",
            put(DbController::set_agent_spec_enabled),
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
            "/api/ports/:port_uri",
            put(DbController::upsert_port).delete(DbController::delete_port),
        )
        .route("/api/ports", get(DbController::list_ports))
        .route(
            "/api/ports/:port_uri/settings",
            get(DbController::list_port_settings),
        )
        .route(
            "/api/ports/:port_uri/settings/:key",
            get(DbController::get_port_setting)
                .put(DbController::upsert_port_setting)
                .delete(DbController::delete_port_setting),
        )
        .route(
            "/api/ports/:port_uri/bindings",
            get(DbController::list_port_bindings),
        )
        .route(
            "/api/ports/:port_uri/bindings/:conversation_key",
            get(DbController::get_port_binding)
                .put(DbController::upsert_port_binding)
                .delete(DbController::delete_port_binding),
        )
        .route(
            "/api/ports/:port_uri/sessions/:session_id/context",
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
