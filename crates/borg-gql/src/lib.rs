mod context;
mod scalars;
mod sdl;

use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use async_graphql::futures_util::{SinkExt, StreamExt, future};
use async_graphql::http::{ALL_WEBSOCKET_PROTOCOLS, GraphiQLSource, WebSocketProtocols, WsMessage};
use async_graphql::{Request, Response, Schema};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{Extension, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use borg_db::BorgDb;
use borg_memory::MemoryStore;
use tokio::net::TcpListener;
use tracing::info;

pub use context::BorgGqlData;
pub use scalars::{JsonValue, UriScalar};
use sdl::{MutationRoot, QueryRoot, SubscriptionRoot};

const DEFAULT_GQL_BIND_ADDR: &str = "127.0.0.1:4008";

/// GraphQL schema type used by Borg clients.
pub type BorgGqlSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Creates a ready-to-serve GraphQL schema.
pub fn build_schema(db: BorgDb, memory: MemoryStore) -> BorgGqlSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(BorgGqlData::new(db, memory))
        .limit_depth(12)
        .limit_complexity(4_000)
        .finish()
}

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
            schema: build_schema(db, memory),
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
        uri: Uri,
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
                WsMessage::Text(text) => Message::Text(text.into()),
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

impl Deref for BorgGqlServer {
    type Target = BorgGqlSchema;

    fn deref(&self) -> &Self::Target {
        &self.schema
    }
}

#[cfg(test)]
mod tests;
