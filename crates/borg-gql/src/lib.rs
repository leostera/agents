mod context;
mod scalars;
mod sdl;

use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_graphql::futures_util::{SinkExt, StreamExt, future};
use async_graphql::http::{ALL_WEBSOCKET_PROTOCOLS, GraphiQLSource, WebSocketProtocols, WsMessage};
use async_graphql::{Request, Response, Schema};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{Extension, State, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri as AxumUri, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use borg_core::Uri;
use borg_db::BorgDb;
use borg_exec::{
    BorgCommand, BorgInput, BorgMessage, BorgRuntime, BorgSupervisor, HttpContext, PortContext,
};
use borg_memory::MemoryStore;
use borg_ports::BorgPortsSupervisor;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

pub use context::BorgGqlData;
pub use scalars::{JsonValue, UriScalar};
use sdl::{MutationRoot, QueryRoot, SubscriptionRoot};

const DEFAULT_GQL_BIND_ADDR: &str = "127.0.0.1:4008";
const HEALTH_STATUS_OK: &str = "ok";
const HTTP_PORT_NAME: &str = "http";
const HTTP_HELP_TEXT: &str = "Available commands: /help, /start, /model [model_name], /participants, /context, /reset, /compact";
const HTTP_START_GREETING: &str = "Borg is online. Send a message to start.";
const MODEL_COMMAND_USAGE: &str = "Usage: /model [model_name]";
const BORG_ACTOR_ID_HEADER: &str = "x-borg-actor-id";

/// GraphQL schema type used by Borg clients.
pub type BorgGqlSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Self-contained GraphQL server container.
#[derive(Clone)]
pub struct BorgGqlServer {
    schema: BorgGqlSchema,
    bind: String,
}

impl BorgGqlServer {
    /// Creates a GraphQL server from runtime stores.
    pub fn new(db: BorgDb, memory: MemoryStore) -> Self {
        Self {
            schema: Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
                .data(BorgGqlData::new(db, memory))
                .limit_depth(100)
                .limit_complexity(4_000)
                .finish(),
            bind: DEFAULT_GQL_BIND_ADDR.to_string(),
        }
    }

    /// Overrides the bind address used by [`BorgGqlServer::run`].
    pub fn with_bind(mut self, bind: impl Into<String>) -> Self {
        self.bind = bind.into();
        self
    }

    /// Returns a cloneable schema handle for integration with other servers.
    pub fn schema(&self) -> BorgGqlSchema {
        self.schema.clone()
    }

    /// Builds an Axum router exposing `/gql`, `/gql/ws`, and `/gql/graphiql`.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/gql", get(Self::graphql_get).post(Self::graphql_post))
            .route("/gql/ws", get(Self::graphql_ws))
            .route("/gql/graphiql", get(Self::graphiql))
            .layer(Extension(self.schema.clone()))
            .layer(cors_layer())
    }

    /// Runs the GraphQL service as a standalone Axum server.
    pub async fn run(self) -> Result<()> {
        let listener = TcpListener::bind(&self.bind).await?;
        info!(
            target: "borg_gql",
            address = %self.bind,
            "graphql server listening"
        );
        axum::serve(listener, self.router()).await?;
        Ok(())
    }

    async fn graphql_post(
        Extension(schema): Extension<BorgGqlSchema>,
        Json(request): Json<Request>,
    ) -> Json<Response> {
        Json(schema.execute(request).await)
    }

    async fn graphql_get(
        Extension(schema): Extension<BorgGqlSchema>,
        uri: AxumUri,
    ) -> impl IntoResponse {
        match async_graphql::http::parse_query_string(uri.query().unwrap_or_default()) {
            Ok(request) => Json(schema.execute(request).await).into_response(),
            Err(err) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        }
    }

    async fn graphql_ws(
        ws: WebSocketUpgrade,
        headers: HeaderMap,
        Extension(schema): Extension<BorgGqlSchema>,
    ) -> impl IntoResponse {
        let Some(protocol) = headers
            .get(header::SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            .and_then(|protocols| {
                protocols
                    .split(',')
                    .find_map(|protocol| WebSocketProtocols::from_str(protocol.trim()).ok())
            })
        else {
            return StatusCode::BAD_REQUEST.into_response();
        };

        ws.protocols(ALL_WEBSOCKET_PROTOCOLS)
            .on_upgrade(move |socket| Self::serve_ws(socket, schema, protocol))
            .into_response()
    }

    async fn serve_ws(socket: WebSocket, schema: BorgGqlSchema, protocol: WebSocketProtocols) {
        let (mut sink, stream) = socket.split();
        let input = stream
            .take_while(|message| future::ready(message.is_ok()))
            .map(Result::unwrap)
            .filter_map(|message| {
                future::ready(match message {
                    Message::Text(text) => Some(text.into_bytes()),
                    Message::Binary(binary) => Some(binary.to_vec()),
                    _ => None,
                })
            });

        let mut stream = async_graphql::http::WebSocket::new(schema, input, protocol)
            .keepalive_timeout(Duration::from_secs(30))
            .map(|message| match message {
                WsMessage::Text(text) => Message::Text(text),
                WsMessage::Close(code, status) => Message::Close(Some(CloseFrame {
                    code,
                    reason: status.into(),
                })),
            });

        while let Some(message) = stream.next().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    }

    async fn graphiql() -> Html<String> {
        Html(
            GraphiQLSource::build()
                .endpoint("/gql")
                .subscription_endpoint("/gql/ws")
                .title("Borg GraphQL")
                .finish(),
        )
    }
}

#[derive(Clone)]
struct RuntimeHttpState {
    db: BorgDb,
    supervisor: Arc<BorgSupervisor>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpPortRequest {
    pub user_key: String,
    pub text: String,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
struct ValidatedHttpPortRequest {
    user_id: Uri,
    text: String,
    actor_id: Option<Uri>,
    metadata: Option<Value>,
}

#[derive(Debug, Clone)]
enum HttpPortInput {
    LocalReply(String),
    Forward(BorgInput),
}

/// Combined HTTP server that exposes GraphQL endpoints plus basic runtime endpoints.
pub struct BorgHttpServer {
    bind: String,
    gql_server: BorgGqlServer,
    state: RuntimeHttpState,
    ports_supervisor: BorgPortsSupervisor,
}

impl BorgHttpServer {
    pub fn new(bind: String, runtime: Arc<BorgRuntime>, supervisor: Arc<BorgSupervisor>) -> Self {
        let gql_server = BorgGqlServer::new(runtime.db.clone(), runtime.memory.clone());
        let ports_supervisor = BorgPortsSupervisor::new(runtime.clone(), supervisor.clone());
        Self {
            bind,
            gql_server,
            state: RuntimeHttpState {
                db: runtime.db.clone(),
                supervisor,
            },
            ports_supervisor,
        }
    }

    pub fn router(&self) -> Router {
        let gql_router: Router<RuntimeHttpState> = self.gql_server.router().with_state(());

        Router::new()
            .route("/", get(Self::index))
            .route("/health", get(Self::health))
            .route("/ports/http", post(Self::ports_http))
            .merge(gql_router)
            .with_state(self.state.clone())
            .layer(cors_layer())
    }

    pub async fn run(self) -> Result<()> {
        let router = self.router();
        let bind = self.bind;
        let ports_supervisor = self.ports_supervisor;
        let ports_task = tokio::spawn(async move {
            if let Err(err) = ports_supervisor.start().await {
                tracing::error!(
                    target: "borg_gql",
                    error = %err,
                    "ports supervisor stopped unexpectedly"
                );
            }
        });

        let listener = TcpListener::bind(&bind).await?;
        info!(target: "borg_gql", address = %bind, "http server listening");

        let shutdown = async {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(
                    target: "borg_gql",
                    error = %err,
                    "failed waiting for ctrl-c signal"
                );
            }
            info!(target: "borg_gql", "received ctrl-c, shutting down");
        };

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await?;
        ports_task.abort();
        Ok(())
    }

    async fn index() -> impl IntoResponse {
        Json(json!({
            "name": "borg-gql",
            "status": HEALTH_STATUS_OK,
            "graphiql": "/gql/graphiql"
        }))
    }

    async fn health() -> impl IntoResponse {
        Json(json!({ "status": HEALTH_STATUS_OK }))
    }

    async fn ports_http(
        State(state): State<RuntimeHttpState>,
        _headers: HeaderMap,
        Json(payload): Json<HttpPortRequest>,
    ) -> impl IntoResponse {
        let validated = match validate_port_request(payload) {
            Ok(value) => value,
            Err(err) => return err,
        };

        let conversation_key = validated.user_id.clone();

        let input = match resolve_http_port_input(&validated.text) {
            Ok(value) => value,
            Err(err) => return bad_request(err),
        };

        let body = match input {
            HttpPortInput::LocalReply(reply) => json!({
                "actor_id": serde_json::Value::Null,
                "reply": reply,
                "tool_calls": [],
            }),
            HttpPortInput::Forward(forward_input) => {
                let default_actor_id = match state.db.get_port(HTTP_PORT_NAME).await {
                    Ok(Some(port)) => port.assigned_actor_id,
                    Ok(None) => None,
                    Err(err) => return internal_error(err),
                };

                let actor_id = match state
                    .db
                    .resolve_port_actor(
                        HTTP_PORT_NAME,
                        &conversation_key,
                        validated.actor_id.as_ref(),
                        default_actor_id.as_ref(),
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(err) => return internal_error(err),
                };

                let output = match state
                    .supervisor
                    .call(BorgMessage {
                        actor_id,
                        user_id: validated.user_id,
                        input: forward_input,
                        port_context: validated
                            .metadata
                            .as_ref()
                            .and_then(|value| {
                                serde_json::from_value::<PortContext>(value.clone()).ok()
                            })
                            .unwrap_or(PortContext::Http(HttpContext::default())),
                    })
                    .await
                {
                    Ok(value) => value,
                    Err(err) => return internal_error(err),
                };

                json!({
                    "actor_id": output.actor_id,
                    "reply": output.reply,
                    "tool_calls": output
                        .tool_calls
                        .into_iter()
                        .map(|call| {
                            json!({
                                "tool_name": call.tool_name,
                                "arguments": call.arguments,
                                "output": call.output,
                            })
                        })
                        .collect::<Vec<_>>(),
                })
            }
        };

        let actor_id_header = body
            .as_object()
            .and_then(|obj| obj.get("actor_id"))
            .and_then(Value::as_str)
            .and_then(|actor_id| HeaderValue::from_str(actor_id).ok());

        let mut response = (StatusCode::OK, Json(body)).into_response();
        if let Some(value) = actor_id_header {
            response.headers_mut().insert(BORG_ACTOR_ID_HEADER, value);
        }
        response
    }
}

fn validate_port_request(
    payload: HttpPortRequest,
) -> std::result::Result<ValidatedHttpPortRequest, axum::response::Response> {
    let user_id = match Uri::parse(&payload.user_key) {
        Ok(value) => value,
        Err(_) => return Err(bad_request("user_key must be a valid URI")),
    };

    let actor_id = match payload.actor_id {
        Some(raw) => match Uri::parse(&raw) {
            Ok(value) => Some(value),
            Err(_) => return Err(bad_request("actor_id must be a valid URI")),
        },
        None => None,
    };

    let text = payload.text.trim().to_string();
    if text.is_empty() {
        return Err(bad_request("text is required"));
    }

    Ok(ValidatedHttpPortRequest {
        user_id,
        text,
        actor_id,
        metadata: payload.metadata,
    })
}

fn resolve_http_port_input(text: &str) -> std::result::Result<HttpPortInput, String> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return Ok(HttpPortInput::Forward(BorgInput::Chat {
            text: text.to_string(),
        }));
    }

    let mut parts = trimmed.split_whitespace();
    let token = parts.next().unwrap_or_default();
    let command = token
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let args: Vec<String> = parts.map(ToOwned::to_owned).collect();

    match command.as_str() {
        "help" => Ok(HttpPortInput::LocalReply(HTTP_HELP_TEXT.to_string())),
        "start" => Ok(HttpPortInput::LocalReply(HTTP_START_GREETING.to_string())),
        "model" => parse_model_command_action(&args),
        "participants" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ParticipantsList,
        ))),
        "context" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ContextDump,
        ))),
        "reset" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ResetContext,
        ))),
        "compact" => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::CompactContext,
        ))),
        "" => Err("empty command".to_string()),
        _ => Err(format!("unknown command: /{command}")),
    }
}

fn parse_model_command_action(args: &[String]) -> std::result::Result<HttpPortInput, String> {
    match args {
        [] => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ModelShowCurrent,
        ))),
        [model] if !model.trim().is_empty() => Ok(HttpPortInput::Forward(BorgInput::Command(
            BorgCommand::ModelSet {
                model: model.trim().to_string(),
            },
        ))),
        [..] => Err(MODEL_COMMAND_USAGE.to_string()),
    }
}

fn bad_request(message: impl Into<String>) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
        .into_response()
}

fn internal_error(err: impl std::fmt::Display) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": err.to_string() })),
    )
        .into_response()
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .expose_headers([header::HeaderName::from_static(BORG_ACTOR_ID_HEADER)])
}

impl Deref for BorgGqlServer {
    type Target = BorgGqlSchema;

    fn deref(&self) -> &Self::Target {
        &self.schema
    }
}
