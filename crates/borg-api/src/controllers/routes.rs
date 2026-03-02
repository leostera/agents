use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::{get, post, put},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::actors::ActorsController;
use super::apps::AppsController;
use super::behaviors::BehaviorsController;
use super::db::DbController;
use super::port_actor_bindings::PortActorBindingsController;
use super::providers::ProvidersController;
use super::system::SystemController;
use crate::AppState;

pub(crate) fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(SystemController::ui_dashboard))
        .route("/dashboard", get(SystemController::ui_dashboard))
        .route("/health", get(SystemController::health))
        .route("/ports/http", post(SystemController::ports_http))
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
        .route(
            "/api/taskgraph/tasks",
            get(DbController::list_taskgraph_tasks).post(DbController::create_taskgraph_task),
        )
        .route(
            "/api/taskgraph/tasks/:task_uri",
            get(DbController::get_taskgraph_task).patch(DbController::update_taskgraph_task_fields),
        )
        .route(
            "/api/taskgraph/tasks/:task_uri/status",
            put(DbController::set_taskgraph_task_status),
        )
        .route(
            "/api/taskgraph/tasks/:task_uri/comments",
            get(DbController::list_taskgraph_comments),
        )
        .route(
            "/api/taskgraph/tasks/:task_uri/events",
            get(DbController::list_taskgraph_events),
        )
        .route(
            "/api/taskgraph/tasks/:task_uri/children",
            get(DbController::list_taskgraph_children),
        )
        .route(
            "/api/clockwork/jobs",
            get(DbController::list_clockwork_jobs).post(DbController::create_clockwork_job),
        )
        .route(
            "/api/clockwork/jobs/:job_id",
            get(DbController::get_clockwork_job).patch(DbController::update_clockwork_job),
        )
        .route(
            "/api/clockwork/jobs/:job_id/pause",
            put(DbController::pause_clockwork_job),
        )
        .route(
            "/api/clockwork/jobs/:job_id/resume",
            put(DbController::resume_clockwork_job),
        )
        .route(
            "/api/clockwork/jobs/:job_id/cancel",
            put(DbController::cancel_clockwork_job),
        )
        .route("/api/providers", get(ProvidersController::list_providers))
        .route(
            "/api/providers/:provider",
            get(ProvidersController::get_provider)
                .put(ProvidersController::upsert_provider)
                .delete(ProvidersController::delete_provider),
        )
        .route(
            "/api/providers/:provider/models",
            get(ProvidersController::list_provider_models),
        )
        .route("/api/apps", get(AppsController::list_apps))
        .route(
            "/api/apps/:app_id",
            get(AppsController::get_app)
                .put(AppsController::upsert_app)
                .delete(AppsController::delete_app),
        )
        .route(
            "/api/apps/:app_id/capabilities",
            get(AppsController::list_app_capabilities),
        )
        .route(
            "/api/apps/:app_id/capabilities/:capability_id",
            get(AppsController::get_app_capability)
                .put(AppsController::upsert_app_capability)
                .delete(AppsController::delete_app_capability),
        )
        .route(
            "/api/apps/:app_id/connections",
            get(AppsController::list_app_connections),
        )
        .route(
            "/api/apps/:app_id/connections/:connection_id",
            get(AppsController::get_app_connection)
                .put(AppsController::upsert_app_connection)
                .delete(AppsController::delete_app_connection),
        )
        .route(
            "/api/apps/:app_id/secrets",
            get(AppsController::list_app_secrets),
        )
        .route(
            "/api/apps/:app_id/secrets/:secret_id",
            get(AppsController::get_app_secret)
                .put(AppsController::upsert_app_secret)
                .delete(AppsController::delete_app_secret),
        )
        .route(
            "/api/apps/:app_id/oauth/start",
            post(borg_apps::oauth_start::<AppState>),
        )
        .route(
            "/oauth/:provider/callback",
            get(borg_apps::oauth_provider_callback::<AppState>),
        )
        .route(
            "/api/providers/openai/device-code/start",
            post(ProvidersController::start_openai_device_code),
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
        .route("/api/behaviors", get(BehaviorsController::list_behaviors))
        .route(
            "/api/behaviors/:behavior_id",
            get(BehaviorsController::get_behavior)
                .put(BehaviorsController::upsert_behavior)
                .delete(BehaviorsController::delete_behavior),
        )
        .route("/api/actors", get(ActorsController::list_actors))
        .route(
            "/api/agents/specs/:agent_id",
            get(DbController::get_agent_spec)
                .put(DbController::upsert_agent_spec)
                .delete(DbController::delete_agent_spec),
        )
        .route(
            "/api/actors/:actor_id",
            get(ActorsController::get_actor)
                .put(ActorsController::upsert_actor)
                .delete(ActorsController::delete_actor),
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
            "/api/ports/:port_uri/actor-bindings",
            get(PortActorBindingsController::list_port_actor_bindings),
        )
        .route(
            "/api/ports/:port_uri/actor-bindings/:conversation_key",
            get(PortActorBindingsController::get_port_actor_binding)
                .put(PortActorBindingsController::upsert_port_actor_binding)
                .delete(PortActorBindingsController::delete_port_actor_binding),
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
