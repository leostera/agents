use std::collections::BTreeMap;
use std::time::Duration;

use async_graphql::futures_util::stream::{self, BoxStream};
use async_graphql::futures_util::{Stream, StreamExt};
use async_graphql::{
    Context, Description, Enum, Error, ErrorExtensions, InputObject, InputValueError,
    InputValueResult, Interface, Object, Result as GqlResult, Scalar, ScalarType, Schema,
    SimpleObject, Subscription, Value,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use borg_core::{Entity, EntityPropValue, Uri};
use borg_db::{
    ActorRecord, AppCapabilityRecord, AppConnectionRecord, AppRecord, AppSecretRecord,
    BehaviorRecord, BorgDb, ClockworkJobRecord, ClockworkJobRunRecord, CreateClockworkJobInput,
    PolicyRecord, PolicyUseRecord, PortRecord, ProviderRecord, SessionMessageRecord, SessionRecord,
    UpdateClockworkJobInput, UserRecord,
};
use borg_memory::{FactArity, FactRecord, FactValue, MemoryStore, SearchQuery, Uri as MemoryUri};
use borg_taskgraph::{
    CreateTaskInput, EventRecord, ListParams, TaskGraphStore, TaskPatch, TaskRecord, TaskStatus,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

/// GraphQL schema type used by Borg clients.
pub type BorgGqlSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

const DEFAULT_PAGE_SIZE: usize = 25;
const MAX_PAGE_SIZE: usize = 200;
const DEFAULT_SUBSCRIPTION_POLL_MS: u64 = 500;
const MIN_SUBSCRIPTION_POLL_MS: u64 = 100;
const MAX_SUBSCRIPTION_POLL_MS: u64 = 5_000;

/// Runtime context for GraphQL resolvers.
#[derive(Clone)]
pub struct BorgGqlData {
    db: BorgDb,
    memory: MemoryStore,
    default_page_size: usize,
    max_page_size: usize,
}

impl BorgGqlData {
    /// Creates a new GraphQL resolver context.
    pub fn new(db: BorgDb, memory: MemoryStore) -> Self {
        Self {
            db,
            memory,
            default_page_size: DEFAULT_PAGE_SIZE,
            max_page_size: MAX_PAGE_SIZE,
        }
    }

    fn normalize_first(&self, first: Option<i32>) -> GqlResult<usize> {
        let raw = first.unwrap_or(self.default_page_size as i32);
        if raw <= 0 {
            return Err(gql_error_with_code(
                "first must be greater than zero",
                "BAD_REQUEST",
            ));
        }
        Ok((raw as usize).min(self.max_page_size))
    }
}

/// Creates a ready-to-serve GraphQL schema.
pub fn build_schema(db: BorgDb, memory: MemoryStore) -> BorgGqlSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(BorgGqlData::new(db, memory))
        .limit_depth(12)
        .limit_complexity(4_000)
        .finish()
}

/// Scalar wrapper around `borg_core::Uri`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UriScalar(pub Uri);

impl From<Uri> for UriScalar {
    fn from(value: Uri) -> Self {
        Self(value)
    }
}

impl From<UriScalar> for Uri {
    fn from(value: UriScalar) -> Self {
        value.0
    }
}

#[Scalar(name = "Uri")]
impl ScalarType for UriScalar {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(raw) => Uri::parse(&raw)
                .map(Self)
                .map_err(|err| InputValueError::custom(err.to_string())),
            other => Err(InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

/// Transitional scalar for fields that still map to legacy JSON columns.
#[derive(Clone, Debug, PartialEq)]
pub struct JsonValue(pub serde_json::Value);

#[Scalar(name = "JsonValue")]
impl ScalarType for JsonValue {
    fn parse(value: Value) -> InputValueResult<Self> {
        let as_json = value
            .into_json()
            .map_err(|err| InputValueError::custom(err.to_string()))?;
        Ok(Self(as_json))
    }

    fn to_value(&self) -> Value {
        Value::from_json(self.0.clone()).unwrap_or(Value::Null)
    }
}

#[derive(SimpleObject, Clone)]
/// Cursor pagination metadata shared by all `*Connection` types.
///
/// Usage notes:
/// - Reuse `endCursor` as the next request's `after` argument.
/// - Stop paginating when `hasNextPage` is `false`.
///
/// Example:
/// ```graphql
/// {
///   actors(first: 20) {
///     pageInfo { hasNextPage endCursor }
///   }
/// }
/// ```
struct PageInfo {
    /// Whether more pages are available after `endCursor`.
    has_next_page: bool,
    /// Opaque cursor for fetching the next page.
    end_cursor: Option<String>,
}

macro_rules! connection_types {
    ($edge:ident, $conn:ident, $node_ty:ty) => {
        #[derive(SimpleObject, Clone)]
        /// Cursor edge wrapper for connection pagination.
        struct $edge {
            /// Opaque edge cursor to pass back into `after`.
            cursor: String,
            /// Materialized node for this edge.
            node: $node_ty,
        }

        #[derive(SimpleObject, Clone)]
        /// Standard GraphQL connection wrapper.
        struct $conn {
            /// Returned edges for the current page.
            edges: Vec<$edge>,
            /// Pagination state for the current page.
            page_info: PageInfo,
        }
    };
}

connection_types!(ActorEdge, ActorConnection, ActorObject);
connection_types!(BehaviorEdge, BehaviorConnection, BehaviorObject);
connection_types!(SessionEdge, SessionConnection, SessionObject);
connection_types!(
    SessionMessageEdge,
    SessionMessageConnection,
    SessionMessageObject
);
connection_types!(PortEdge, PortConnection, PortObject);
connection_types!(PortBindingEdge, PortBindingConnection, PortBindingObject);
connection_types!(
    PortActorBindingEdge,
    PortActorBindingConnection,
    PortActorBindingObject
);
connection_types!(ProviderEdge, ProviderConnection, ProviderObject);
connection_types!(AppEdge, AppListConnection, AppObject);
connection_types!(
    AppCapabilityEdge,
    AppCapabilityConnection,
    AppCapabilityObject
);
connection_types!(
    AppExternalConnectionEdge,
    AppExternalConnectionConnection,
    AppExternalConnectionObject
);
connection_types!(AppSecretEdge, AppSecretConnection, AppSecretObject);
connection_types!(ClockworkJobEdge, ClockworkJobConnection, ClockworkJobObject);
connection_types!(
    ClockworkJobRunEdge,
    ClockworkJobRunConnection,
    ClockworkJobRunObject
);
connection_types!(TaskEdge, TaskConnection, TaskObject);
connection_types!(TaskCommentEdge, TaskCommentConnection, TaskCommentObject);
connection_types!(TaskEventEdge, TaskEventConnection, TaskEventObject);
connection_types!(MemoryEntityEdge, MemoryEntityConnection, MemoryEntityObject);
connection_types!(MemoryFactEdge, MemoryFactConnection, MemoryFactObject);
connection_types!(PolicyEdge, PolicyConnection, PolicyObject);
connection_types!(PolicyUseEdge, PolicyUseConnection, PolicyUseObject);
connection_types!(UserEdge, UserConnection, UserObject);

fn gql_error_with_code(message: impl Into<String>, code: &'static str) -> Error {
    Error::new(message).extend_with(|_, e| {
        e.set("code", code);
    })
}

fn map_anyhow(err: anyhow::Error) -> Error {
    let message = err.to_string();
    let normalized = message.to_ascii_lowercase();
    let code = if normalized.contains("not_found") || normalized.contains("not found") {
        "NOT_FOUND"
    } else if normalized.contains("conflict") || normalized.contains("already exists") {
        "CONFLICT"
    } else if normalized.contains("invalid")
        || normalized.contains("validation")
        || normalized.contains("required")
        || normalized.contains("bad request")
    {
        "BAD_REQUEST"
    } else {
        "INTERNAL"
    };
    gql_error_with_code(message, code)
}

fn ctx_data<'a>(ctx: &'a Context<'_>) -> GqlResult<&'a BorgGqlData> {
    ctx.data::<BorgGqlData>()
        .map_err(|_| gql_error_with_code("missing BorgGqlData context", "INTERNAL"))
}

fn parse_core_uri(raw: &str) -> GqlResult<Uri> {
    Uri::parse(raw).map_err(map_anyhow)
}

fn parse_memory_uri(raw: &str) -> GqlResult<MemoryUri> {
    MemoryUri::parse(raw.to_string()).map_err(map_anyhow)
}

fn to_memory_uri(uri: &Uri) -> GqlResult<MemoryUri> {
    parse_memory_uri(uri.as_str())
}

fn from_memory_uri(uri: &MemoryUri) -> GqlResult<UriScalar> {
    parse_core_uri(uri.as_str()).map(UriScalar)
}

fn encode_offset_cursor(offset: usize) -> String {
    URL_SAFE_NO_PAD.encode(format!("offset:{offset}"))
}

fn decode_offset_cursor(after: Option<&str>) -> GqlResult<usize> {
    let Some(after) = after else {
        return Ok(0);
    };

    let bytes = URL_SAFE_NO_PAD
        .decode(after)
        .map_err(|_| gql_error_with_code("invalid cursor", "BAD_REQUEST"))?;
    let decoded = String::from_utf8(bytes)
        .map_err(|_| gql_error_with_code("invalid cursor", "BAD_REQUEST"))?;
    let Some(raw) = decoded.strip_prefix("offset:") else {
        return Err(gql_error_with_code("invalid cursor", "BAD_REQUEST"));
    };
    let value = raw
        .parse::<usize>()
        .map_err(|_| gql_error_with_code("invalid cursor", "BAD_REQUEST"))?;
    Ok(value + 1)
}

fn apply_offset_pagination<T>(
    items: Vec<T>,
    start: usize,
    first: usize,
) -> (Vec<(usize, T)>, bool) {
    let mut page = items
        .into_iter()
        .skip(start)
        .take(first + 1)
        .collect::<Vec<_>>();
    let has_next = page.len() > first;
    if has_next {
        let _ = page.pop();
    }
    let out = page
        .into_iter()
        .enumerate()
        .map(|(index, item)| (start + index, item))
        .collect::<Vec<_>>();
    (out, has_next)
}

fn parse_uri_kind(uri: &Uri) -> Option<&str> {
    uri.as_str().split(':').nth(1)
}

fn parse_uri_id(uri: &Uri) -> Option<&str> {
    uri.as_str().split(':').nth(2)
}

fn provider_uri(provider: &str) -> GqlResult<Uri> {
    Uri::from_parts("borg", "provider", Some(provider)).map_err(map_anyhow)
}

fn parse_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn encode_task_cursor(created_at: &str, id: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{created_at}|{id}"))
}

fn normalize_poll_interval_ms(raw: Option<i32>) -> GqlResult<u64> {
    let value = raw.unwrap_or(DEFAULT_SUBSCRIPTION_POLL_MS as i32);
    if value <= 0 {
        return Err(gql_error_with_code(
            "pollIntervalMs must be greater than zero",
            "BAD_REQUEST",
        ));
    }

    Ok((value as u64).clamp(MIN_SUBSCRIPTION_POLL_MS, MAX_SUBSCRIPTION_POLL_MS))
}

async fn resolve_session_stream_start_index(
    data: &BorgGqlData,
    session_id: &Uri,
    after_message_index: Option<i64>,
) -> GqlResult<i64> {
    let session_exists = data
        .db
        .get_session(session_id)
        .await
        .map_err(map_anyhow)?
        .is_some();
    if !session_exists {
        return Err(gql_error_with_code("session not found", "NOT_FOUND"));
    }

    if let Some(after) = after_message_index {
        if after < -1 {
            return Err(gql_error_with_code(
                "afterMessageIndex must be greater than or equal to -1",
                "BAD_REQUEST",
            ));
        }
        return Ok(after.saturating_add(1));
    }

    let total = data
        .db
        .count_session_messages(session_id)
        .await
        .map_err(map_anyhow)?;
    Ok(total as i64)
}

fn session_message_subscription_stream(
    data: BorgGqlData,
    session_id: Uri,
    start_index: i64,
    poll_interval_ms: u64,
) -> impl Stream<Item = GqlResult<SessionMessageObject>> {
    let ticker = tokio::time::interval(Duration::from_millis(poll_interval_ms));

    stream::unfold(
        (data, session_id, start_index, ticker),
        |(data, session_id, mut next_index, mut ticker)| async move {
            loop {
                ticker.tick().await;

                let total = match data.db.count_session_messages(&session_id).await {
                    Ok(value) => value as i64,
                    Err(err) => {
                        return Some((
                            Err(map_anyhow(err)),
                            (data, session_id, next_index, ticker),
                        ));
                    }
                };

                if next_index >= total {
                    continue;
                }

                let index = next_index;
                next_index += 1;

                match data.db.get_session_message(&session_id, index).await {
                    Ok(Some(record)) => {
                        return Some((
                            Ok(SessionMessageObject::new(record)),
                            (data, session_id, next_index, ticker),
                        ));
                    }
                    Ok(None) => continue,
                    Err(err) => {
                        return Some((
                            Err(map_anyhow(err)),
                            (data, session_id, next_index, ticker),
                        ));
                    }
                }
            }
        },
    )
}

#[derive(Clone, Description)]
/// Root query entrypoint for the Borg entity graph.
///
/// Usage notes:
/// - Use `node(id: Uri!)` for generic cross-entity lookup.
/// - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
/// - Connection fields use cursor pagination via `first` + `after`.
/// - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
///
/// Example:
/// ```graphql
/// query {
///   actors(first: 5) {
///     edges { node { id name status } }
///   }
/// }
/// ```
pub struct QueryRoot;

#[Object(use_type_description)]
impl QueryRoot {
    /// Fetches a single graph node by URI and resolves the concrete runtime type.
    ///
    /// Usage notes:
    /// - Works for actor/behavior/session/port/provider/app/task/policy/user/memory entities.
    /// - Use inline fragments to read type-specific fields.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) {
    ///   node(id: $id) {
    ///     id
    ///     ... on Actor { name status }
    ///   }
    /// }
    /// ```
    async fn node(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<Node>> {
        let data = ctx_data(ctx)?;

        match parse_uri_kind(&id.0) {
            Some("actor") => Ok(data
                .db
                .get_actor(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(ActorObject::new)
                .map(Node::from)),
            Some("behavior") => Ok(data
                .db
                .get_behavior(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(BehaviorObject::new)
                .map(Node::from)),
            Some("session") => Ok(data
                .db
                .get_session(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(SessionObject::new)
                .map(Node::from)),
            Some("port") => Ok(data
                .db
                .get_port_by_id(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(PortObject::new)
                .map(Node::from)),
            Some("app") => Ok(data
                .db
                .get_app(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(AppObject::new)
                .map(Node::from)),
            Some("policy") => Ok(data
                .db
                .get_policy(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(PolicyObject::new)
                .map(Node::from)),
            Some("user") => Ok(data
                .db
                .get_user(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(UserObject::new)
                .map(Node::from)),
            Some("task") => {
                let store = TaskGraphStore::new(data.db.clone());
                match store.get_task(id.0.as_str()).await {
                    Ok(task) => Ok(Some(Node::from(TaskObject::try_new(task)?))),
                    Err(err) if err.to_string().contains("not_found") => Ok(None),
                    Err(err) => Err(map_anyhow(err)),
                }
            }
            Some("provider") => {
                let Some(provider_key) = parse_uri_id(&id.0) else {
                    return Ok(None);
                };
                Ok(data
                    .db
                    .get_provider(provider_key)
                    .await
                    .map_err(map_anyhow)?
                    .map(ProviderObject::try_new)
                    .transpose()?
                    .map(Node::from))
            }
            _ => {
                let memory_uri = to_memory_uri(&id.0)?;
                Ok(data
                    .memory
                    .get_entity_uri(&memory_uri)
                    .await
                    .map_err(map_anyhow)?
                    .map(MemoryEntityObject::new)
                    .map(Node::from))
            }
        }
    }

    /// Fetches one actor by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { actor(id: $id) { id name status } }
    /// ```
    async fn actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }

    /// Lists actors ordered by most-recent update.
    ///
    /// Usage notes:
    /// - `first` defaults to 25 and is capped server-side.
    /// - Pass the previous `endCursor` into `after` to paginate.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   actors(first: 10) {
    ///     edges { cursor node { id name } }
    ///     pageInfo { hasNextPage endCursor }
    ///   }
    /// }
    /// ```
    async fn actors(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ActorConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let actors = data.db.list_actors(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(actors, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ActorEdge {
                cursor: encode_offset_cursor(index),
                node: ActorObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ActorConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one behavior by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { behavior(id: $id) { id name preferredProviderId } }
    /// ```
    async fn behavior(
        &self,
        ctx: &Context<'_>,
        id: UriScalar,
    ) -> GqlResult<Option<BehaviorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_behavior(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(BehaviorObject::new))
    }

    /// Lists behaviors ordered by most-recent update.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   behaviors(first: 20) {
    ///     edges { node { id name status requiredCapabilities } }
    ///   }
    /// }
    /// ```
    async fn behaviors(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<BehaviorConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let behaviors = data
            .db
            .list_behaviors(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(behaviors, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| BehaviorEdge {
                cursor: encode_offset_cursor(index),
                node: BehaviorObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(BehaviorConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one session by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { session(id: $id) { id users portId updatedAt } }
    /// ```
    async fn session(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<SessionObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_session(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(SessionObject::new))
    }

    /// Lists sessions ordered by most-recent update.
    ///
    /// Usage notes:
    /// - Optional filters: `portId` and `userId`.
    /// - Use nested `messages` for chat timeline reads.
    ///
    /// Example:
    /// ```graphql
    /// query($port: Uri!) {
    ///   sessions(first: 10, portId: $port) {
    ///     edges { node { id updatedAt } }
    ///   }
    /// }
    /// ```
    async fn sessions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        port_id: Option<UriScalar>,
        user_id: Option<UriScalar>,
    ) -> GqlResult<SessionConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let sessions = data
            .db
            .list_sessions(
                fetch_limit,
                port_id.as_ref().map(|uri| &uri.0),
                user_id.as_ref().map(|uri| &uri.0),
            )
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(sessions, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| SessionEdge {
                cursor: encode_offset_cursor(index),
                node: SessionObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(SessionConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one port by canonical port name (for example `http`, `telegram`).
    ///
    /// Example:
    /// ```graphql
    /// query { port(name: "http") { id name provider enabled } }
    /// ```
    async fn port(&self, ctx: &Context<'_>, name: String) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port(&name)
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

    /// Fetches one port by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { portById(id: $id) { id name allowsGuests } }
    /// ```
    async fn port_by_id(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port_by_id(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

    /// Lists ports ordered by activity.
    ///
    /// Usage notes:
    /// - Includes `activeSessions` and binding relations for routing debugging.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   ports(first: 20) {
    ///     edges { node { name provider activeSessions } }
    ///   }
    /// }
    /// ```
    async fn ports(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PortConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let ports = data.db.list_ports(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(ports, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| PortEdge {
                cursor: encode_offset_cursor(index),
                node: PortObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(PortConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one provider by provider key.
    ///
    /// Example:
    /// ```graphql
    /// query { provider(provider: "openai") { id provider providerKind enabled } }
    /// ```
    async fn provider(
        &self,
        ctx: &Context<'_>,
        provider: String,
    ) -> GqlResult<Option<ProviderObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_provider(&provider)
            .await
            .map_err(map_anyhow)?
            .map(ProviderObject::try_new)
            .transpose()?)
    }

    /// Lists configured model providers.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   providers(first: 10) {
    ///     edges { node { provider providerKind defaultTextModel tokensUsed } }
    ///   }
    /// }
    /// ```
    async fn providers(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ProviderConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let providers = data
            .db
            .list_providers(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(providers, start, first);

        let mut edges = Vec::with_capacity(page.len());
        for (index, record) in page {
            edges.push(ProviderEdge {
                cursor: encode_offset_cursor(index),
                node: ProviderObject::try_new(record)?,
            });
        }

        Ok(ProviderConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one app by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { app(id: $id) { id name slug status } }
    /// ```
    async fn app(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

    /// Fetches one app by slug.
    ///
    /// Example:
    /// ```graphql
    /// query { appBySlug(slug: "github") { id name capabilities(first: 5) { edges { node { name } } } } }
    /// ```
    async fn app_by_slug(&self, ctx: &Context<'_>, slug: String) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app_by_slug(&slug)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

    /// Lists apps available in Borg.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   apps(first: 25) {
    ///     edges { node { id slug authStrategy availableSecrets } }
    ///   }
    /// }
    /// ```
    async fn apps(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<AppListConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let apps = data.db.list_apps(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(apps, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| AppEdge {
                cursor: encode_offset_cursor(index),
                node: AppObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(AppListConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one clockwork job by `jobId`.
    ///
    /// Example:
    /// ```graphql
    /// query { clockworkJob(jobId: "daily-digest") { id status nextRunAt } }
    /// ```
    async fn clockwork_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
    ) -> GqlResult<Option<ClockworkJobObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_clockwork_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .map(ClockworkJobObject::new))
    }

    /// Lists clockwork jobs with optional status filtering.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   clockworkJobs(first: 20, status: "active") {
    ///     edges { node { id kind status runs(first: 5) { edges { node { id firedAt } } } } }
    ///   }
    /// }
    /// ```
    async fn clockwork_jobs(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        status: Option<String>,
    ) -> GqlResult<ClockworkJobConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let jobs = data
            .db
            .list_clockwork_jobs(fetch_limit, status.as_deref())
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(jobs, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ClockworkJobEdge {
                cursor: encode_offset_cursor(index),
                node: ClockworkJobObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ClockworkJobConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one task by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) {
    ///   task(id: $id) {
    ///     id title status
    ///     comments(first: 10) { edges { node { id body } } }
    ///   }
    /// }
    /// ```
    async fn task(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<TaskObject>> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        match store.get_task(id.0.as_str()).await {
            Ok(task) => Ok(Some(TaskObject::try_new(task)?)),
            Err(err) if err.to_string().contains("not_found") => Ok(None),
            Err(err) => Err(map_anyhow(err)),
        }
    }

    /// Lists top-level taskgraph tasks.
    ///
    /// Usage notes:
    /// - Cursor format follows taskgraph ordering (`createdAt`, `id`).
    /// - Traverse children via `Task.children`.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   tasks(first: 15) {
    ///     edges { node { id title status parentUri } }
    ///   }
    /// }
    /// ```
    async fn tasks(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<TaskConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let store = TaskGraphStore::new(data.db.clone());

        let (tasks, next_cursor) = store
            .list_tasks(ListParams {
                cursor: after,
                limit: first,
            })
            .await
            .map_err(map_anyhow)?;

        let mut edges = Vec::with_capacity(tasks.len());
        for task in tasks {
            let cursor = encode_task_cursor(&task.created_at, &task.uri);
            edges.push(TaskEdge {
                cursor,
                node: TaskObject::try_new(task)?,
            });
        }

        Ok(TaskConnection {
            page_info: PageInfo {
                has_next_page: next_cursor.is_some(),
                end_cursor: edges.last().map(|edge| edge.cursor.clone()).or(next_cursor),
            },
            edges,
        })
    }

    /// Fetches one memory entity by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { memoryEntity(id: $id) { id label props { key value { kind text } } } }
    /// ```
    async fn memory_entity(
        &self,
        ctx: &Context<'_>,
        id: UriScalar,
    ) -> GqlResult<Option<MemoryEntityObject>> {
        let data = ctx_data(ctx)?;
        let memory_uri = to_memory_uri(&id.0)?;
        Ok(data
            .memory
            .get_entity_uri(&memory_uri)
            .await
            .map_err(map_anyhow)?
            .map(MemoryEntityObject::new))
    }

    /// Searches memory entities by free text plus optional namespace/kind filters.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   memoryEntities(queryText: "alice", kind: "person", first: 10) {
    ///     edges { node { id label } }
    ///   }
    /// }
    /// ```
    async fn memory_entities(
        &self,
        ctx: &Context<'_>,
        query_text: String,
        ns: Option<String>,
        kind: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<MemoryEntityConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let results = data
            .memory
            .search_query(SearchQuery {
                ns,
                kind,
                name: None,
                query_text: Some(query_text),
                limit: Some(fetch_limit),
            })
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(results.entities, start, first);
        let edges = page
            .into_iter()
            .map(|(index, entity)| MemoryEntityEdge {
                cursor: encode_offset_cursor(index),
                node: MemoryEntityObject::new(entity),
            })
            .collect::<Vec<_>>();

        Ok(MemoryEntityConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one fact row by fact URI string.
    ///
    /// Example:
    /// ```graphql
    /// query($id: String!) { memoryFact(id: $id) { id arity value { kind text } } }
    /// ```
    async fn memory_fact(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> GqlResult<Option<MemoryFactObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .memory
            .get_fact(&id)
            .await
            .map_err(map_anyhow)?
            .map(MemoryFactObject::new))
    }

    /// Lists fact rows with optional entity/field filters.
    ///
    /// Usage notes:
    /// - Set `includeRetracted: true` for audit/replay tooling.
    ///
    /// Example:
    /// ```graphql
    /// query($entity: Uri!) {
    ///   memoryFacts(entityId: $entity, first: 20) {
    ///     edges { node { id field value { kind text reference } } }
    ///   }
    /// }
    /// ```
    async fn memory_facts(
        &self,
        ctx: &Context<'_>,
        entity_id: Option<UriScalar>,
        field_id: Option<UriScalar>,
        include_retracted: Option<bool>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<MemoryFactConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let entity = entity_id
            .as_ref()
            .map(|uri| to_memory_uri(&uri.0))
            .transpose()?;
        let field = field_id
            .as_ref()
            .map(|uri| to_memory_uri(&uri.0))
            .transpose()?;

        let facts = data
            .memory
            .list_facts(
                entity.as_ref(),
                field.as_ref(),
                include_retracted.unwrap_or(false),
                fetch_limit,
            )
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(facts, start, first);
        let edges = page
            .into_iter()
            .map(|(index, fact)| MemoryFactEdge {
                cursor: encode_offset_cursor(index),
                node: MemoryFactObject::new(fact),
            })
            .collect::<Vec<_>>();

        Ok(MemoryFactConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one policy by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { policy(id: $id) { id uses(first: 10) { edges { node { entityId } } } } }
    /// ```
    async fn policy(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<PolicyObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_policy(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(PolicyObject::new))
    }

    /// Lists policies.
    ///
    /// Example:
    /// ```graphql
    /// query { policies(first: 25) { edges { node { id updatedAt } } } }
    /// ```
    async fn policies(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PolicyConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let policies = data
            .db
            .list_policies(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(policies, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| PolicyEdge {
                cursor: encode_offset_cursor(index),
                node: PolicyObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(PolicyConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one user by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { user(id: $id) { id createdAt updatedAt } }
    /// ```
    async fn user(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<UserObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_user(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(UserObject::new))
    }

    /// Lists users.
    ///
    /// Example:
    /// ```graphql
    /// query { users(first: 50) { edges { node { id updatedAt } } } }
    /// ```
    async fn users(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<UserConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let users = data.db.list_users(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(users, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| UserEdge {
                cursor: encode_offset_cursor(index),
                node: UserObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(UserConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// Root mutation entrypoint for control-plane and task/memory writes.
///
/// Usage notes:
/// - Mutations return the written object whenever possible.
/// - URI arguments are strictly validated by the `Uri` scalar.
/// - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
///
/// Example:
/// ```graphql
/// mutation {
///   upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
///     provider
///     enabled
///   }
/// }
/// ```
pub struct MutationRoot;

#[Object(use_type_description)]
impl MutationRoot {
    /// Creates or updates an actor.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!, $behavior: Uri!) {
    ///   upsertActor(input: {
    ///     id: $id
    ///     name: "Planner"
    ///     systemPrompt: "You plan work."
    ///     defaultBehaviorId: $behavior
    ///     status: "RUNNING"
    ///   }) { id name status }
    /// }
    /// ```
    async fn upsert_actor(
        &self,
        ctx: &Context<'_>,
        input: UpsertActorInput,
    ) -> GqlResult<ActorObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_actor(
                &input.id.0,
                &input.name,
                &input.system_prompt,
                &input.default_behavior_id.0,
                &input.status,
            )
            .await
            .map_err(map_anyhow)?;

        let actor = data
            .db
            .get_actor(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("actor not found after upsert", "INTERNAL"))?;

        Ok(ActorObject::new(actor))
    }

    /// Deletes an actor by URI.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) { deleteActor(id: $id) }
    /// ```
    async fn delete_actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data.db.delete_actor(&id.0).await.map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates a behavior.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) {
    ///   upsertBehavior(input: {
    ///     id: $id
    ///     name: "default"
    ///     systemPrompt: "..."
    ///     sessionTurnConcurrency: "serial"
    ///     status: "ACTIVE"
    ///     requiredCapabilities: ["TaskGraph-listTasks"]
    ///   }) { id name status requiredCapabilities }
    /// }
    /// ```
    async fn upsert_behavior(
        &self,
        ctx: &Context<'_>,
        input: UpsertBehaviorInput,
    ) -> GqlResult<BehaviorObject> {
        let data = ctx_data(ctx)?;
        let required = serde_json::Value::Array(
            input
                .required_capabilities
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );

        data.db
            .upsert_behavior(
                &input.id.0,
                &input.name,
                &input.system_prompt,
                input.preferred_provider_id.as_deref(),
                &required,
                &input.session_turn_concurrency,
                &input.status,
            )
            .await
            .map_err(map_anyhow)?;

        let behavior = data
            .db
            .get_behavior(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("behavior not found after upsert", "INTERNAL"))?;

        Ok(BehaviorObject::new(behavior))
    }

    /// Deletes a behavior by URI.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) { deleteBehavior(id: $id) }
    /// ```
    async fn delete_behavior(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data.db.delete_behavior(&id.0).await.map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates a port.
    ///
    /// Usage notes:
    /// - `assignedActorId` is mirrored into `settings.actor_id` for compatibility.
    /// - `settings` must be a JSON object when provided.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   upsertPort(input: {
    ///     name: "http"
    ///     provider: "custom"
    ///     enabled: true
    ///     allowsGuests: true
    ///   }) { id name enabled allowsGuests }
    /// }
    /// ```
    async fn upsert_port(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortInput,
    ) -> GqlResult<PortObject> {
        let data = ctx_data(ctx)?;
        let mut settings = input
            .settings
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            return Err(gql_error_with_code(
                "settings must be a JSON object",
                "BAD_REQUEST",
            ));
        }

        if let Some(actor_id) = &input.assigned_actor_id {
            if let Some(object) = settings.as_object_mut() {
                object.insert(
                    "actor_id".to_string(),
                    serde_json::Value::String(actor_id.0.to_string()),
                );
            }
        } else if let Some(object) = settings.as_object_mut() {
            object.remove("actor_id");
        }

        data.db
            .upsert_port(
                &input.name,
                &input.provider,
                input.enabled,
                input.allows_guests,
                input.assigned_actor_id.as_ref().map(|uri| &uri.0),
                &settings,
            )
            .await
            .map_err(map_anyhow)?;

        let port = data
            .db
            .get_port(&input.name)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("port not found after upsert", "INTERNAL"))?;
        Ok(PortObject::new(port))
    }

    /// Creates or updates a port/session binding.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!, $key: Uri!) {
    ///   upsertPortBinding(input: {
    ///     portName: "telegram"
    ///     conversationKey: $key
    ///     sessionId: $session
    ///   }) { portName conversationKey sessionId }
    /// }
    /// ```
    async fn upsert_port_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortBindingInput,
    ) -> GqlResult<PortBindingObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_port_binding_record(
                &input.port_name,
                &input.conversation_key.0,
                &input.session_id.0,
            )
            .await
            .map_err(map_anyhow)?;

        Ok(PortBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0,
            session_id: input.session_id.0,
        })
    }

    /// Creates or updates a port/actor binding.
    ///
    /// Usage notes:
    /// - Pass `actorId: null` to clear the actor binding.
    ///
    /// Example:
    /// ```graphql
    /// mutation($key: Uri!, $actor: Uri!) {
    ///   upsertPortActorBinding(input: {
    ///     portName: "telegram"
    ///     conversationKey: $key
    ///     actorId: $actor
    ///   }) { portName conversationKey actorId }
    /// }
    /// ```
    async fn upsert_port_actor_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortActorBindingInput,
    ) -> GqlResult<PortActorBindingObject> {
        let data = ctx_data(ctx)?;
        if let Some(actor_id) = input.actor_id.as_ref() {
            data.db
                .upsert_port_actor_binding(&input.port_name, &input.conversation_key.0, &actor_id.0)
                .await
                .map_err(map_anyhow)?;
        } else {
            data.db
                .clear_port_actor_binding(&input.port_name, &input.conversation_key.0)
                .await
                .map_err(map_anyhow)?;
        }

        let actor_id = data
            .db
            .get_port_actor_binding(&input.port_name, &input.conversation_key.0)
            .await
            .map_err(map_anyhow)?;

        Ok(PortActorBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0,
            actor_id,
        })
    }

    /// Creates or updates a provider.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   upsertProvider(input: {
    ///     provider: "openrouter"
    ///     providerKind: "openrouter"
    ///     apiKey: "sk-***"
    ///     enabled: true
    ///     defaultTextModel: "openai/gpt-4.1-mini"
    ///   }) { provider providerKind enabled }
    /// }
    /// ```
    async fn upsert_provider(
        &self,
        ctx: &Context<'_>,
        input: UpsertProviderInput,
    ) -> GqlResult<ProviderObject> {
        let data = ctx_data(ctx)?;
        let provider_kind = input
            .provider_kind
            .as_deref()
            .unwrap_or(input.provider.as_str());

        data.db
            .upsert_provider_with_kind(
                &input.provider,
                provider_kind,
                input.api_key.as_deref(),
                input.base_url.as_deref(),
                input.enabled,
                input.default_text_model.as_deref(),
                input.default_audio_model.as_deref(),
            )
            .await
            .map_err(map_anyhow)?;

        let provider = data
            .db
            .get_provider(&input.provider)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("provider not found after upsert", "INTERNAL"))?;
        ProviderObject::try_new(provider)
    }

    /// Deletes a provider and associated usage summary.
    ///
    /// Example:
    /// ```graphql
    /// mutation { deleteProvider(provider: "openrouter") }
    /// ```
    async fn delete_provider(&self, ctx: &Context<'_>, provider: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data
            .db
            .delete_provider(&provider)
            .await
            .map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates an app.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) {
    ///   upsertApp(input: {
    ///     id: $id
    ///     name: "GitHub"
    ///     slug: "github"
    ///     description: "GitHub integration"
    ///     status: "ACTIVE"
    ///     builtIn: false
    ///     source: "custom"
    ///     authStrategy: "oauth2"
    ///     availableSecrets: ["GITHUB_TOKEN"]
    ///   }) { id slug status }
    /// }
    /// ```
    async fn upsert_app(&self, ctx: &Context<'_>, input: UpsertAppInput) -> GqlResult<AppObject> {
        let data = ctx_data(ctx)?;
        let auth_config = input
            .auth_config
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));

        data.db
            .upsert_app_with_metadata(
                &input.id.0,
                &input.name,
                &input.slug,
                &input.description,
                &input.status,
                input.built_in,
                &input.source,
                &input.auth_strategy,
                &auth_config,
                &input.available_secrets,
            )
            .await
            .map_err(map_anyhow)?;

        let app = data
            .db
            .get_app(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("app not found after upsert", "INTERNAL"))?;
        Ok(AppObject::new(app))
    }

    /// Creates or updates an app capability.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $cap: Uri!) {
    ///   upsertAppCapability(input: {
    ///     appId: $app
    ///     capabilityId: $cap
    ///     name: "issues.list"
    ///     hint: "List GitHub issues"
    ///     mode: "READ"
    ///     instructions: "Use filters when possible"
    ///     status: "ACTIVE"
    ///   }) { id name status }
    /// }
    /// ```
    async fn upsert_app_capability(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppCapabilityInput,
    ) -> GqlResult<AppCapabilityObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_app_capability(
                &input.app_id.0,
                &input.capability_id.0,
                &input.name,
                &input.hint,
                &input.mode,
                &input.instructions,
                &input.status,
            )
            .await
            .map_err(map_anyhow)?;

        let capability = data
            .db
            .get_app_capability(&input.app_id.0, &input.capability_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("capability not found after upsert", "INTERNAL"))?;

        Ok(AppCapabilityObject::new(capability))
    }

    /// Creates or updates an app connection.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $conn: Uri!, $owner: Uri) {
    ///   upsertAppConnection(input: {
    ///     appId: $app
    ///     connectionId: $conn
    ///     ownerUserId: $owner
    ///     status: "CONNECTED"
    ///   }) { id appId status }
    /// }
    /// ```
    async fn upsert_app_connection(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppConnectionInput,
    ) -> GqlResult<AppExternalConnectionObject> {
        let data = ctx_data(ctx)?;
        let connection_json = input
            .connection
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));

        data.db
            .upsert_app_connection(
                &input.app_id.0,
                &input.connection_id.0,
                input.owner_user_id.as_ref().map(|uri| &uri.0),
                input.provider_account_id.as_deref(),
                input.external_user_id.as_deref(),
                &input.status,
                &connection_json,
            )
            .await
            .map_err(map_anyhow)?;

        let connection = data
            .db
            .get_app_connection(&input.app_id.0, &input.connection_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("connection not found after upsert", "INTERNAL"))?;

        Ok(AppExternalConnectionObject::new(connection))
    }

    /// Creates or updates an app secret.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $secret: Uri!) {
    ///   upsertAppSecret(input: {
    ///     appId: $app
    ///     secretId: $secret
    ///     key: "GITHUB_TOKEN"
    ///     value: "..."
    ///     kind: "token"
    ///   }) { id key kind }
    /// }
    /// ```
    async fn upsert_app_secret(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppSecretInput,
    ) -> GqlResult<AppSecretObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_app_secret(
                &input.app_id.0,
                &input.secret_id.0,
                input.connection_id.as_ref().map(|uri| &uri.0),
                &input.key,
                &input.value,
                &input.kind,
            )
            .await
            .map_err(map_anyhow)?;

        let secret = data
            .db
            .get_app_secret(&input.app_id.0, &input.secret_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("secret not found after upsert", "INTERNAL"))?;

        Ok(AppSecretObject::new(secret))
    }

    /// Creates or updates a session.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!, $user: Uri!, $port: Uri!) {
    ///   upsertSession(input: { sessionId: $id, users: [$user], port: $port }) { id users portId }
    /// }
    /// ```
    async fn upsert_session(
        &self,
        ctx: &Context<'_>,
        input: UpsertSessionInput,
    ) -> GqlResult<SessionObject> {
        let data = ctx_data(ctx)?;
        let users = input.users.into_iter().map(|uri| uri.0).collect::<Vec<_>>();
        data.db
            .upsert_session(&input.session_id.0, &users, &input.port.0)
            .await
            .map_err(map_anyhow)?;

        let session = data
            .db
            .get_session(&input.session_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("session not found after upsert", "INTERNAL"))?;

        Ok(SessionObject::new(session))
    }

    /// Appends a session message.
    ///
    /// Usage notes:
    /// - Prefer typed fields (`messageType`, `role`, `text`) over raw `payload`.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!) {
    ///   appendSessionMessage(input: {
    ///     sessionId: $session
    ///     messageType: "user"
    ///     role: "user"
    ///     text: "Hello"
    ///   }) { id messageIndex messageType role text }
    /// }
    /// ```
    async fn append_session_message(
        &self,
        ctx: &Context<'_>,
        input: AppendSessionMessageInput,
    ) -> GqlResult<SessionMessageObject> {
        let data = ctx_data(ctx)?;
        let payload = build_session_message_payload(&SessionMessageInput {
            message_type: input.message_type.clone(),
            role: input.role.clone(),
            text: input.text.clone(),
            payload: input.payload.clone(),
        })?;

        let index = data
            .db
            .append_session_message(&input.session_id.0, &payload)
            .await
            .map_err(map_anyhow)?;

        let message = data
            .db
            .get_session_message(&input.session_id.0, index)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("message not found after append", "INTERNAL"))?;

        Ok(SessionMessageObject::new(message))
    }

    /// Updates an existing session message.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!) {
    ///   patchSessionMessage(input: {
    ///     sessionId: $session
    ///     messageIndex: 0
    ///     message: { messageType: "user", role: "user", text: "Updated text" }
    ///   }) { id messageIndex text }
    /// }
    /// ```
    async fn patch_session_message(
        &self,
        ctx: &Context<'_>,
        input: PatchSessionMessageInput,
    ) -> GqlResult<SessionMessageObject> {
        let data = ctx_data(ctx)?;
        let payload = build_session_message_payload(&input.message)?;

        data.db
            .update_session_message(&input.session_id.0, input.message_index, &payload)
            .await
            .map_err(map_anyhow)?;

        let message = data
            .db
            .get_session_message(&input.session_id.0, input.message_index)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("session message not found", "NOT_FOUND"))?;

        Ok(SessionMessageObject::new(message))
    }

    /// Creates a clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation($actor: Uri!, $session: Uri!) {
    ///   createClockworkJob(input: {
    ///     jobId: "daily-standup"
    ///     kind: "cron"
    ///     actorId: $actor
    ///     sessionId: $session
    ///     messageType: "user"
    ///     payload: { text: "Daily standup" }
    ///     scheduleSpec: { cron: "0 9 * * 1-5" }
    ///   }) { id status nextRunAt }
    /// }
    /// ```
    async fn create_clockwork_job(
        &self,
        ctx: &Context<'_>,
        input: CreateClockworkJobInputGql,
    ) -> GqlResult<ClockworkJobObject> {
        let data = ctx_data(ctx)?;
        let create = CreateClockworkJobInput {
            job_id: input.job_id.clone(),
            kind: input.kind,
            target_actor_id: input.actor_id.0.to_string(),
            target_session_id: input.session_id.0.to_string(),
            message_type: input.message_type,
            payload: input.payload.0,
            headers: input
                .headers
                .map(|value| value.0)
                .unwrap_or_else(|| json!({})),
            schedule_spec: input.schedule_spec.0,
            next_run_at: input.next_run_at,
        };

        data.db
            .create_clockwork_job(&create)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_clockwork_job(&input.job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("clockwork job not found", "INTERNAL"))?;

        Ok(ClockworkJobObject::new(job))
    }

    /// Updates mutable clockwork job fields.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   updateClockworkJob(
    ///     jobId: "daily-standup",
    ///     patch: { scheduleSpec: { cron: "0 10 * * 1-5" } }
    ///   ) { id status scheduleSpec }
    /// }
    /// ```
    async fn update_clockwork_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
        patch: UpdateClockworkJobInputGql,
    ) -> GqlResult<ClockworkJobObject> {
        let data = ctx_data(ctx)?;
        let update = UpdateClockworkJobInput {
            kind: patch.kind,
            target_actor_id: patch.actor_id.map(|uri| uri.0.to_string()),
            target_session_id: patch.session_id.map(|uri| uri.0.to_string()),
            message_type: patch.message_type,
            payload: patch.payload.map(|value| value.0),
            headers: patch.headers.map(|value| value.0),
            schedule_spec: patch.schedule_spec.map(|value| value.0),
            next_run_at: patch.next_run_at.map(Some),
        };

        data.db
            .update_clockwork_job(&job_id, &update)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_clockwork_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("clockwork job not found", "NOT_FOUND"))?;

        Ok(ClockworkJobObject::new(job))
    }

    /// Pauses an active clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { pauseClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn pause_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "paused")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Resumes a paused clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { resumeClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn resume_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "active")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Cancels a clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { cancelClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn cancel_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "cancelled")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Creates a task in the taskgraph store.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!, $creator: Uri!, $assignee: Uri!) {
    ///   createTask(input: {
    ///     sessionUri: $session
    ///     creatorAgentId: $creator
    ///     assigneeAgentId: $assignee
    ///     title: "Ship borg-gql docs"
    ///     description: "Document all schema entrypoints"
    ///   }) { id title status }
    /// }
    /// ```
    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());

        let created = store
            .create_task(
                input.session_uri.0.as_str(),
                input.creator_agent_id.0.as_str(),
                CreateTaskInput {
                    title: input.title,
                    description: input.description,
                    definition_of_done: input.definition_of_done,
                    assignee_agent_id: input.assignee_agent_id.0.to_string(),
                    parent_uri: input.parent_uri.map(|uri| uri.0.to_string()),
                    blocked_by: input
                        .blocked_by
                        .into_iter()
                        .map(|uri| uri.0.to_string())
                        .collect(),
                    references: input
                        .references
                        .into_iter()
                        .map(|uri| uri.0.to_string())
                        .collect(),
                    labels: input.labels,
                },
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(created)
    }

    /// Updates mutable task text fields.
    ///
    /// Example:
    /// ```graphql
    /// mutation($task: Uri!, $session: Uri!) {
    ///   updateTask(input: {
    ///     taskId: $task
    ///     sessionUri: $session
    ///     title: "Updated title"
    ///   }) { id title updatedAt }
    /// }
    /// ```
    async fn update_task(
        &self,
        ctx: &Context<'_>,
        input: UpdateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .update_task_fields(
                input.session_uri.0.as_str(),
                input.task_id.0.as_str(),
                TaskPatch {
                    title: input.title,
                    description: input.description,
                    definition_of_done: input.definition_of_done,
                },
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(task)
    }

    /// Moves a task to a new allowed status.
    ///
    /// Usage notes:
    /// - Auth/session constraints follow taskgraph rules (assignee/reviewer checks).
    ///
    /// Example:
    /// ```graphql
    /// mutation($task: Uri!, $session: Uri!) {
    ///   setTaskStatus(input: { taskId: $task, sessionUri: $session, status: DOING }) {
    ///     id
    ///     status
    ///   }
    /// }
    /// ```
    async fn set_task_status(
        &self,
        ctx: &Context<'_>,
        input: SetTaskStatusInput,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .set_task_status(
                input.session_uri.0.as_str(),
                input.task_id.0.as_str(),
                input.status.into(),
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(task)
    }

    /// Placeholder runtime wrapper; enabled after `borg-api` integration.
    ///
    /// Usage notes:
    /// - Currently returns `BAD_REQUEST`.
    /// - Keep frontend contracts ready for upcoming runtime integration.
    ///
    /// Example:
    /// ```graphql
    /// mutation($actor: Uri!, $session: Uri!, $user: Uri!) {
    ///   runActorChat(input: {
    ///     actorId: $actor
    ///     sessionId: $session
    ///     userId: $user
    ///     text: "Summarize pending tasks"
    ///   }) { ok message }
    /// }
    /// ```
    async fn run_actor_chat(&self, _input: RunActorChatInput) -> GqlResult<RunActorChatResult> {
        Err(gql_error_with_code(
            "runActorChat is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }

    /// Placeholder runtime wrapper; enabled after `borg-api` integration.
    ///
    /// Usage notes:
    /// - Currently returns `BAD_REQUEST`.
    /// - Expected to mirror `POST /ports/http` behavior in a later phase.
    ///
    /// Example:
    /// ```graphql
    /// mutation($user: Uri!) {
    ///   runPortHttp(input: { userId: $user, text: "Hello" }) { ok message }
    /// }
    /// ```
    async fn run_port_http(&self, _input: RunPortHttpInput) -> GqlResult<RunPortHttpResult> {
        Err(gql_error_with_code(
            "runPortHttp is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }
}

#[derive(Clone, Description)]
/// Root subscription entrypoint for real-time Borg streams.
///
/// Usage notes:
/// - Subscription transport is expected to run over WebSockets (`graphql-transport-ws`).
/// - Use `sessionChat` for full timeline streaming.
/// - Use `sessionNotifications` for notification-friendly filtered events.
///
/// Example:
/// ```graphql
/// subscription($session: Uri!) {
///   sessionChat(sessionId: $session) {
///     messageIndex
///     messageType
///     role
///     text
///   }
/// }
/// ```
pub struct SubscriptionRoot;

#[Subscription(use_type_description)]
impl SubscriptionRoot {
    /// Streams new messages from a session timeline as they are appended.
    ///
    /// Usage notes:
    /// - When `afterMessageIndex` is omitted, the stream starts from "now" (tail-follow mode).
    /// - Provide `afterMessageIndex` to replay from a known point.
    /// - `pollIntervalMs` is clamped to safe server bounds.
    ///
    /// Example:
    /// ```graphql
    /// subscription($session: Uri!, $after: Int) {
    ///   sessionChat(sessionId: $session, afterMessageIndex: $after, pollIntervalMs: 500) {
    ///     id
    ///     messageIndex
    ///     messageType
    ///     role
    ///     text
    ///   }
    /// }
    /// ```
    async fn session_chat(
        &self,
        ctx: &Context<'_>,
        session_id: UriScalar,
        after_message_index: Option<i64>,
        poll_interval_ms: Option<i32>,
    ) -> BoxStream<'static, GqlResult<SessionMessageObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start =
                resolve_session_stream_start_index(&data, &session_id.0, after_message_index)
                    .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;

            Ok::<_, Error>(session_message_subscription_stream(
                data,
                session_id.0,
                start,
                poll_ms,
            ))
        }
        .await;

        match setup {
            Ok(stream) => stream.boxed(),
            Err(err) => stream::once(async move { Err(err) }).boxed(),
        }
    }

    /// Streams session notifications derived from new timeline messages.
    ///
    /// Usage notes:
    /// - By default, user-authored messages are filtered out.
    /// - Set `includeUserMessages: true` to receive all roles.
    ///
    /// Example:
    /// ```graphql
    /// subscription($session: Uri!) {
    ///   sessionNotifications(sessionId: $session) {
    ///     id
    ///     kind
    ///     title
    ///     text
    ///     sessionMessage { messageIndex messageType role }
    ///   }
    /// }
    /// ```
    async fn session_notifications(
        &self,
        ctx: &Context<'_>,
        session_id: UriScalar,
        after_message_index: Option<i64>,
        poll_interval_ms: Option<i32>,
        include_user_messages: Option<bool>,
    ) -> BoxStream<'static, GqlResult<SessionNotificationObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start =
                resolve_session_stream_start_index(&data, &session_id.0, after_message_index)
                    .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;
            let include_users = include_user_messages.unwrap_or(false);

            let stream = session_message_subscription_stream(data, session_id.0, start, poll_ms)
                .filter_map(move |item| async move {
                    match item {
                        Ok(message) => {
                            let parsed = message.parsed();
                            let is_user = parsed.role.as_deref() == Some("user");
                            if is_user && !include_users {
                                return None;
                            }
                            Some(Ok(SessionNotificationObject::from_message(message)))
                        }
                        Err(err) => Some(Err(err)),
                    }
                });

            Ok::<_, Error>(stream)
        }
        .await;

        match setup {
            Ok(stream) => stream.boxed(),
            Err(err) => stream::once(async move { Err(err) }).boxed(),
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Notification kind classification for `SessionNotification`.
enum SessionNotificationKind {
    /// Assistant-authored response message.
    AssistantReply,
    /// Tool call or tool result activity.
    ToolActivity,
    /// Session lifecycle or system event message.
    SessionEvent,
    /// Fallback generic message classification.
    Message,
}

#[derive(SimpleObject, Clone, Description)]
/// Notification payload projected from a session message.
///
/// Usage notes:
/// - Notifications are fully typed and include the underlying `sessionMessage`.
/// - `kind` is derived from `messageType`/`role`.
struct SessionNotificationObject {
    /// Stable notification identifier (`sessionId:messageIndex`).
    id: String,
    /// Kind classification for UI routing and badges.
    kind: SessionNotificationKind,
    /// Short user-facing title.
    title: String,
    /// Session URI this notification belongs to.
    session_id: UriScalar,
    /// Source message index.
    message_index: i64,
    /// Source message type.
    message_type: String,
    /// Source role, when available.
    role: Option<String>,
    /// Source text content, when available.
    text: Option<String>,
    /// Source message timestamp.
    created_at: DateTime<Utc>,
    /// Full underlying message object.
    session_message: SessionMessageObject,
}

impl SessionNotificationObject {
    fn from_message(message: SessionMessageObject) -> Self {
        let parsed = message.parsed();
        let (kind, title) = match parsed.message_type.as_str() {
            "assistant" => (
                SessionNotificationKind::AssistantReply,
                "Assistant reply".to_string(),
            ),
            "tool_call" => (
                SessionNotificationKind::ToolActivity,
                "Tool call requested".to_string(),
            ),
            "tool_result" => (
                SessionNotificationKind::ToolActivity,
                "Tool result received".to_string(),
            ),
            "session_event" => (
                SessionNotificationKind::SessionEvent,
                "Session event".to_string(),
            ),
            _ => (
                SessionNotificationKind::Message,
                "Session message".to_string(),
            ),
        };

        Self {
            id: format!(
                "{}:{}",
                message.record.session_id, message.record.message_index
            ),
            kind,
            title,
            session_id: message.record.session_id.clone().into(),
            message_index: message.record.message_index,
            message_type: parsed.message_type,
            role: parsed.role,
            text: parsed.text,
            created_at: message.record.created_at,
            session_message: message,
        }
    }
}

/// Input payload for `upsertActor`.
///
/// Example:
/// `{ id: "borg:actor:planner", name: "Planner", defaultBehaviorId: "borg:behavior:default", status: "RUNNING" }`
#[derive(InputObject)]
struct UpsertActorInput {
    /// Stable actor URI (`borg:actor:*`).
    id: UriScalar,
    /// Human-readable actor name.
    name: String,
    /// System prompt used when running this actor.
    system_prompt: String,
    /// Behavior URI linked as actor default.
    default_behavior_id: UriScalar,
    /// Actor lifecycle status (for example `RUNNING`).
    status: String,
}

/// Input payload for `upsertBehavior`.
///
/// Usage notes:
/// - `requiredCapabilities` should contain runtime tool/capability names.
///
/// Example:
/// `{ id: "borg:behavior:default", name: "default", sessionTurnConcurrency: "serial", status: "ACTIVE" }`
#[derive(InputObject)]
struct UpsertBehaviorInput {
    /// Stable behavior URI (`borg:behavior:*`).
    id: UriScalar,
    /// Human-readable behavior label.
    name: String,
    /// System prompt attached to this behavior.
    system_prompt: String,
    /// Preferred provider key (for example `openai`, `openrouter`).
    preferred_provider_id: Option<String>,
    /// Capability names required by this behavior.
    #[graphql(default)]
    required_capabilities: Vec<String>,
    /// Turn execution policy (for example `serial`).
    session_turn_concurrency: String,
    /// Behavior lifecycle status.
    status: String,
}

/// Input payload for `upsertPort`.
///
/// Example:
/// `{ name: "telegram", provider: "telegram", enabled: true, allowsGuests: false }`
#[derive(InputObject)]
struct UpsertPortInput {
    /// Port name (for example `http`, `telegram`).
    name: String,
    /// Port provider/transport family.
    provider: String,
    /// Whether the port can ingest traffic.
    enabled: bool,
    /// Whether unauthenticated users are accepted.
    allows_guests: bool,
    /// Optional default actor for this port.
    assigned_actor_id: Option<UriScalar>,
    /// Optional JSON settings object.
    settings: Option<JsonValue>,
}

/// Input payload for `upsertPortBinding`.
///
/// Example:
/// `{ portName: "telegram", conversationKey: "borg:conversation:123", sessionId: "borg:session:s1" }`
#[derive(InputObject)]
struct UpsertPortBindingInput {
    /// Port name (`http`, `telegram`, ...).
    port_name: String,
    /// Stable conversation key for ingress routing.
    conversation_key: UriScalar,
    /// Target long-lived session URI.
    session_id: UriScalar,
}

/// Input payload for `upsertPortActorBinding`.
///
/// Example:
/// `{ portName: "telegram", conversationKey: "borg:conversation:123", actorId: "borg:actor:planner" }`
#[derive(InputObject)]
struct UpsertPortActorBindingInput {
    /// Port name (`http`, `telegram`, ...).
    port_name: String,
    /// Conversation key used for routing.
    conversation_key: UriScalar,
    /// Actor URI or `null` to clear binding.
    actor_id: Option<UriScalar>,
}

/// Input payload for `upsertProvider`.
///
/// Example:
/// `{ provider: "openai", providerKind: "openai", enabled: true, defaultTextModel: "gpt-4.1-mini" }`
#[derive(InputObject)]
struct UpsertProviderInput {
    /// Provider key (`openai`, `openrouter`, ...).
    provider: String,
    /// Provider family/kind. Defaults to `provider` when omitted.
    provider_kind: Option<String>,
    /// API key/token for this provider.
    api_key: Option<String>,
    /// Optional base URL override.
    base_url: Option<String>,
    /// Enable or disable the provider.
    enabled: Option<bool>,
    /// Preferred default model for text generation.
    default_text_model: Option<String>,
    /// Preferred default model for audio/transcription.
    default_audio_model: Option<String>,
}

/// Input payload for `upsertApp`.
///
/// Example:
/// `{ id: "borg:app:github", name: "GitHub", slug: "github", status: "ACTIVE", authStrategy: "oauth2" }`
#[derive(InputObject)]
struct UpsertAppInput {
    /// Stable app URI (`borg:app:*`).
    id: UriScalar,
    /// Human-readable app name.
    name: String,
    /// URL-safe app slug.
    slug: String,
    /// Description shown in clients/admin screens.
    description: String,
    /// App lifecycle status.
    status: String,
    /// Whether this app is bundled by Borg.
    #[graphql(default)]
    built_in: bool,
    /// App source origin (`builtin`, `custom`, ...).
    source: String,
    /// Authentication strategy (`none`, `oauth2`, ...).
    auth_strategy: String,
    /// Transitional JSON auth config.
    auth_config: Option<JsonValue>,
    /// Secret keys this app expects to read.
    #[graphql(default)]
    available_secrets: Vec<String>,
}

/// Input payload for `upsertAppCapability`.
///
/// Example:
/// `{ appId: "borg:app:github", capabilityId: "borg:capability:issues-list", name: "issues.list", mode: "READ" }`
#[derive(InputObject)]
struct UpsertAppCapabilityInput {
    /// Parent app URI.
    app_id: UriScalar,
    /// Capability URI.
    capability_id: UriScalar,
    /// Capability display name.
    name: String,
    /// Short hint for UI/LLM tooltips.
    hint: String,
    /// Capability mode (`READ`, `WRITE`, ...).
    mode: String,
    /// Detailed execution instructions for this capability.
    instructions: String,
    /// Capability lifecycle status.
    status: String,
}

/// Input payload for `upsertAppConnection`.
///
/// Example:
/// `{ appId: "borg:app:github", connectionId: "borg:app-connection:octocat", status: "CONNECTED" }`
#[derive(InputObject)]
struct UpsertAppConnectionInput {
    /// Parent app URI.
    app_id: UriScalar,
    /// Stable connection URI.
    connection_id: UriScalar,
    /// Owning user URI.
    owner_user_id: Option<UriScalar>,
    /// Provider account identifier.
    provider_account_id: Option<String>,
    /// External user/account identifier.
    external_user_id: Option<String>,
    /// Connection lifecycle status.
    status: String,
    /// Transitional JSON metadata for this connection.
    connection: Option<JsonValue>,
}

/// Input payload for `upsertAppSecret`.
///
/// Example:
/// `{ appId: "borg:app:github", secretId: "borg:app-secret:token", key: "GITHUB_TOKEN", kind: "token" }`
#[derive(InputObject)]
struct UpsertAppSecretInput {
    /// Parent app URI.
    app_id: UriScalar,
    /// Stable secret URI.
    secret_id: UriScalar,
    /// Optional scoped connection URI.
    connection_id: Option<UriScalar>,
    /// Secret key name.
    key: String,
    /// Secret value.
    value: String,
    /// Secret kind (`token`, `password`, ...).
    kind: String,
}

/// Input payload for `upsertSession`.
///
/// Example:
/// `{ sessionId: "borg:session:s1", users: ["borg:user:u1"], port: "borg:port:http" }`
#[derive(InputObject)]
struct UpsertSessionInput {
    /// Session URI (`borg:session:*`).
    session_id: UriScalar,
    /// Participant user URIs.
    users: Vec<UriScalar>,
    /// Owning ingress port URI.
    port: UriScalar,
}

/// Typed message patch input used by session message mutations.
///
/// Usage notes:
/// - Prefer typed fields over `payload` for new clients.
/// - At least one field should be set.
#[derive(InputObject, Clone)]
struct SessionMessageInput {
    /// Message type (`user`, `assistant`, `tool_call`, ...).
    message_type: Option<String>,
    /// Logical role (`user`, `assistant`, `tool`, ...).
    role: Option<String>,
    /// Primary human-readable content.
    text: Option<String>,
    /// Transitional raw payload.
    payload: Option<JsonValue>,
}

/// Input payload for `appendSessionMessage`.
///
/// Example:
/// `{ sessionId: "borg:session:s1", messageType: "user", role: "user", text: "Hello" }`
#[derive(InputObject)]
struct AppendSessionMessageInput {
    /// Target session URI.
    session_id: UriScalar,
    /// Message type (`user`, `assistant`, ...).
    message_type: Option<String>,
    /// Logical role (`user`, `assistant`, ...).
    role: Option<String>,
    /// Message text content.
    text: Option<String>,
    /// Transitional raw payload.
    payload: Option<JsonValue>,
}

/// Input payload for `patchSessionMessage`.
///
/// Example:
/// `{ sessionId: "borg:session:s1", messageIndex: 0, message: { text: "Updated" } }`
#[derive(InputObject)]
struct PatchSessionMessageInput {
    /// Target session URI.
    session_id: UriScalar,
    /// Zero-based message index in the session timeline.
    message_index: i64,
    /// Replacement message payload.
    message: SessionMessageInput,
}

/// Input payload for `createClockworkJob`.
///
/// Example:
/// `{ jobId: "daily-digest", kind: "cron", actorId: "borg:actor:planner", sessionId: "borg:session:s1" }`
#[derive(InputObject)]
struct CreateClockworkJobInputGql {
    /// Stable job identifier.
    job_id: String,
    /// Scheduler kind (`cron`, ...).
    kind: String,
    /// Actor URI executed by this job.
    actor_id: UriScalar,
    /// Session URI used as job context.
    session_id: UriScalar,
    /// Message envelope type.
    message_type: String,
    /// Job payload (transitional JSON).
    payload: JsonValue,
    /// Optional job headers (transitional JSON).
    headers: Option<JsonValue>,
    /// Schedule specification (transitional JSON).
    schedule_spec: JsonValue,
    /// Optional explicit first run timestamp (RFC3339 string).
    next_run_at: Option<String>,
}

/// Input payload for `updateClockworkJob`.
///
/// Usage notes:
/// - Only pass fields that need to change.
/// - `nextRunAt` set to `null` clears the existing schedule override.
#[derive(InputObject)]
struct UpdateClockworkJobInputGql {
    /// New scheduler kind.
    kind: Option<String>,
    /// New target actor URI.
    actor_id: Option<UriScalar>,
    /// New target session URI.
    session_id: Option<UriScalar>,
    /// New message type.
    message_type: Option<String>,
    /// New payload (transitional JSON).
    payload: Option<JsonValue>,
    /// New headers (transitional JSON).
    headers: Option<JsonValue>,
    /// New schedule spec (transitional JSON).
    schedule_spec: Option<JsonValue>,
    /// New next run timestamp (RFC3339 string).
    next_run_at: Option<String>,
}

/// Input payload for `createTask`.
///
/// Example:
/// `{ sessionUri: "borg:session:s1", creatorAgentId: "borg:actor:creator", assigneeAgentId: "borg:actor:assignee", title: "Ship docs" }`
#[derive(InputObject)]
struct CreateTaskInputGql {
    /// Session URI authoring the task create event.
    session_uri: UriScalar,
    /// Creator actor URI.
    creator_agent_id: UriScalar,
    /// Short task title.
    title: String,
    /// Task description/body.
    #[graphql(default)]
    description: String,
    /// Completion criteria.
    #[graphql(default)]
    definition_of_done: String,
    /// Assignee actor URI.
    assignee_agent_id: UriScalar,
    /// Parent task URI when creating a subtask.
    parent_uri: Option<UriScalar>,
    /// Task URIs that block this task.
    #[graphql(default)]
    blocked_by: Vec<UriScalar>,
    /// Related entity/task URIs.
    #[graphql(default)]
    references: Vec<UriScalar>,
    /// User-defined labels.
    #[graphql(default)]
    labels: Vec<String>,
}

/// Input payload for `updateTask`.
///
/// Usage notes:
/// - This mutation only patches text fields.
/// - Leave fields as `null` to keep previous values.
#[derive(InputObject)]
struct UpdateTaskInputGql {
    /// Task URI to patch.
    task_id: UriScalar,
    /// Session URI authoring the update event.
    session_uri: UriScalar,
    /// Optional new title.
    title: Option<String>,
    /// Optional new description.
    description: Option<String>,
    /// Optional new definition of done.
    definition_of_done: Option<String>,
}

/// Input payload for `setTaskStatus`.
///
/// Example:
/// `{ taskId: "borg:task:t1", sessionUri: "borg:session:s-assignee", status: DOING }`
#[derive(InputObject)]
struct SetTaskStatusInput {
    /// Task URI to transition.
    task_id: UriScalar,
    /// Session URI authoring the status transition.
    session_uri: UriScalar,
    /// Target status value.
    status: TaskStatusValue,
}

/// Placeholder input payload for `runActorChat`.
///
/// Usage notes:
/// - Reserved for future runtime integration.
#[derive(InputObject)]
struct RunActorChatInput {
    /// Actor URI to execute.
    actor_id: UriScalar,
    /// Session URI context.
    session_id: UriScalar,
    /// User URI authoring the request.
    user_id: UriScalar,
    /// User text to send.
    text: String,
}

/// Placeholder input payload for `runPortHttp`.
///
/// Usage notes:
/// - Reserved for future runtime integration.
#[derive(InputObject)]
struct RunPortHttpInput {
    /// User URI authoring the request.
    user_id: UriScalar,
    /// User text to send.
    text: String,
    /// Optional existing session URI.
    session_id: Option<UriScalar>,
    /// Optional explicit actor URI override.
    actor_id: Option<UriScalar>,
}

fn build_session_message_payload(input: &SessionMessageInput) -> GqlResult<serde_json::Value> {
    if let Some(payload) = &input.payload {
        return Ok(payload.0.clone());
    }

    let mut object = BTreeMap::new();

    if let Some(message_type) = &input.message_type {
        object.insert(
            "type".to_string(),
            serde_json::Value::String(message_type.clone()),
        );
    }

    if let Some(role) = &input.role {
        object.insert("role".to_string(), serde_json::Value::String(role.clone()));
    }

    if let Some(text) = &input.text {
        object.insert(
            "content".to_string(),
            serde_json::Value::String(text.clone()),
        );
    }

    if object.is_empty() {
        return Err(gql_error_with_code(
            "message requires either payload or typed fields",
            "BAD_REQUEST",
        ));
    }

    Ok(serde_json::Value::Object(object.into_iter().collect()))
}

impl From<AppendSessionMessageInput> for SessionMessageInput {
    fn from(value: AppendSessionMessageInput) -> Self {
        Self {
            message_type: value.message_type,
            role: value.role,
            text: value.text,
            payload: value.payload,
        }
    }
}

#[derive(Clone, Description)]
/// GraphQL `Actor` object.
///
/// Usage notes:
/// - Represents a runnable actor spec (`borg:actor:*`).
/// - Use `defaultBehavior` and `sessions` for common workspace screens.
///
/// Example:
/// ```graphql
/// { actor(id: "borg:actor:planner") { id name defaultBehavior { id name } } }
/// ```
struct ActorObject {
    record: ActorRecord,
}

impl ActorObject {
    fn new(record: ActorRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Actor", use_type_description)]
impl ActorObject {
    /// Stable actor URI.
    async fn id(&self) -> UriScalar {
        self.record.actor_id.clone().into()
    }

    async fn name(&self) -> &str {
        &self.record.name
    }

    async fn system_prompt(&self) -> &str {
        &self.record.system_prompt
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    /// Default behavior used by this actor.
    ///
    /// Example:
    /// ```graphql
    /// { actor(id: "borg:actor:planner") { defaultBehavior { id name preferredProviderId } } }
    /// ```
    async fn default_behavior(&self, ctx: &Context<'_>) -> GqlResult<Option<BehaviorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_behavior(&self.record.default_behavior_id)
            .await
            .map_err(map_anyhow)?
            .map(BehaviorObject::new))
    }

    /// Sessions this actor has participated in.
    ///
    /// Usage notes:
    /// - Backed by actor mailbox activity.
    ///
    /// Example:
    /// ```graphql
    /// { actor(id: "borg:actor:planner") { sessions(first: 5) { edges { node { id updatedAt } } } } }
    /// ```
    async fn sessions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<SessionConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let ids = data
            .db
            .list_actor_sessions(&self.record.actor_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(ids, start, first);
        let mut edges = Vec::new();
        for (index, session_id) in page {
            if let Some(session) = data.db.get_session(&session_id).await.map_err(map_anyhow)? {
                edges.push(SessionEdge {
                    cursor: encode_offset_cursor(index),
                    node: SessionObject::new(session),
                });
            }
        }

        Ok(SessionConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `Behavior` object attached to actors.
///
/// Usage notes:
/// - Behaviors define prompts, preferred provider, and required capabilities.
/// - `requiredCapabilities` maps directly to runtime capability names.
///
/// Example:
/// ```graphql
/// { behavior(id: "borg:behavior:default") { id name requiredCapabilities preferredProvider { provider } } }
/// ```
struct BehaviorObject {
    record: BehaviorRecord,
}

impl BehaviorObject {
    fn new(record: BehaviorRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Behavior", use_type_description)]
impl BehaviorObject {
    async fn id(&self) -> UriScalar {
        self.record.behavior_id.clone().into()
    }

    async fn name(&self) -> &str {
        &self.record.name
    }

    async fn system_prompt(&self) -> &str {
        &self.record.system_prompt
    }

    async fn preferred_provider_id(&self) -> Option<&str> {
        self.record.preferred_provider_id.as_deref()
    }

    /// Provider preferred by this behavior.
    ///
    /// Example:
    /// ```graphql
    /// { behavior(id: "borg:behavior:default") { preferredProvider { provider providerKind } } }
    /// ```
    async fn preferred_provider(&self, ctx: &Context<'_>) -> GqlResult<Option<ProviderObject>> {
        let Some(provider_id) = self.record.preferred_provider_id.as_deref() else {
            return Ok(None);
        };

        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_provider(provider_id)
            .await
            .map_err(map_anyhow)?
            .map(ProviderObject::try_new)
            .transpose()?)
    }

    /// Capability names required by this behavior.
    ///
    /// Example:
    /// ```graphql
    /// { behavior(id: "borg:behavior:default") { requiredCapabilities } }
    /// ```
    async fn required_capabilities(&self) -> Vec<String> {
        parse_string_array(&self.record.required_capabilities_json)
    }

    async fn session_turn_concurrency(&self) -> &str {
        &self.record.session_turn_concurrency
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `Session` object representing a conversation timeline.
///
/// Usage notes:
/// - Session is the primary unit for chat/task execution context.
/// - Traverse into `messages` for timeline rows and `port` for ingress metadata.
///
/// Example:
/// ```graphql
/// { session(id: "borg:session:s1") { id users portId messages(first: 5) { edges { node { messageIndex text } } } } }
/// ```
struct SessionObject {
    record: SessionRecord,
}

impl SessionObject {
    fn new(record: SessionRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Session", use_type_description)]
impl SessionObject {
    async fn id(&self) -> UriScalar {
        self.record.session_id.clone().into()
    }

    async fn users(&self) -> Vec<UriScalar> {
        self.record
            .users
            .iter()
            .cloned()
            .map(UriScalar)
            .collect::<Vec<_>>()
    }

    async fn port_id(&self) -> UriScalar {
        self.record.port.clone().into()
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    /// Port metadata associated with this session.
    ///
    /// Example:
    /// ```graphql
    /// { session(id: "borg:session:s1") { port { id name provider } } }
    /// ```
    async fn port(&self, ctx: &Context<'_>) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;

        if let Some(port) = data
            .db
            .get_port_by_id(&self.record.port)
            .await
            .map_err(map_anyhow)?
        {
            return Ok(Some(PortObject::new(port)));
        }

        let fallback_name = parse_uri_id(&self.record.port).map(str::to_string);
        if let Some(port_name) = fallback_name {
            return Ok(data
                .db
                .get_port(&port_name)
                .await
                .map_err(map_anyhow)?
                .map(PortObject::new));
        }

        Ok(None)
    }

    /// Messages ordered by `messageIndex` ascending.
    ///
    /// Usage notes:
    /// - Use `after` with connection cursor for incremental timeline sync.
    ///
    /// Example:
    /// ```graphql
    /// { session(id: "borg:session:s1") { messages(first: 20) { edges { node { messageIndex role text } } } } }
    /// ```
    async fn messages(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<SessionMessageConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;

        let total = data
            .db
            .count_session_messages(&self.record.session_id)
            .await
            .map_err(map_anyhow)?;
        let end_exclusive = (start + first + 1).min(total);

        let mut messages = Vec::new();
        for index in start..end_exclusive {
            if let Some(message) = data
                .db
                .get_session_message(&self.record.session_id, index as i64)
                .await
                .map_err(map_anyhow)?
            {
                messages.push(message);
            }
        }

        let has_next_page = start + first < total;
        let edges = messages
            .into_iter()
            .enumerate()
            .map(|(offset, record)| SessionMessageEdge {
                cursor: encode_offset_cursor(start + offset),
                node: SessionMessageObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(SessionMessageConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `SessionMessage` object for one indexed timeline row.
///
/// Usage notes:
/// - Prefer `messageType`, `role`, and `text` over deprecated `payload`.
/// - `messageIndex` is zero-based and monotonic within a session.
///
/// Example:
/// ```graphql
/// { session(id: "borg:session:s1") { messages(first: 5) { edges { node { messageIndex messageType role text } } } } }
/// ```
struct SessionMessageObject {
    record: SessionMessageRecord,
}

impl SessionMessageObject {
    fn new(record: SessionMessageRecord) -> Self {
        Self { record }
    }

    fn parsed(&self) -> ParsedSessionMessage {
        parse_session_message(&self.record.payload)
    }
}

#[Object(name = "SessionMessage", use_type_description)]
impl SessionMessageObject {
    async fn id(&self) -> UriScalar {
        self.record.message_id.clone().into()
    }

    async fn session_id(&self) -> UriScalar {
        self.record.session_id.clone().into()
    }

    async fn message_index(&self) -> i64 {
        self.record.message_index
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn message_type(&self) -> String {
        self.parsed().message_type
    }

    async fn role(&self) -> Option<String> {
        self.parsed().role
    }

    async fn text(&self) -> Option<String> {
        self.parsed().text
    }

    #[graphql(
        deprecation = "Legacy JSON payload. Prefer typed fields (`messageType`, `role`, `text`)."
    )]
    async fn payload(&self) -> JsonValue {
        JsonValue(self.record.payload.clone())
    }
}

#[derive(Clone, Description)]
/// GraphQL `Port` object for ingress/egress routing configuration.
///
/// Usage notes:
/// - Ports bind external channels (`http`, `telegram`, ...) to session routing.
/// - `bindings` and `actorBindings` expose live routing maps.
///
/// Example:
/// ```graphql
/// { port(name: "telegram") { id provider enabled bindings(first: 5) { edges { node { conversationKey sessionId } } } } }
/// ```
struct PortObject {
    record: PortRecord,
}

impl PortObject {
    fn new(record: PortRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Port", use_type_description)]
impl PortObject {
    async fn id(&self) -> UriScalar {
        self.record.port_id.clone().into()
    }

    async fn name(&self) -> &str {
        &self.record.port_name
    }

    async fn provider(&self) -> &str {
        &self.record.provider
    }

    async fn enabled(&self) -> bool {
        self.record.enabled
    }

    async fn allows_guests(&self) -> bool {
        self.record.allows_guests
    }

    async fn assigned_actor_id(&self) -> Option<UriScalar> {
        self.record.assigned_actor_id.clone().map(UriScalar)
    }

    async fn active_sessions(&self) -> u64 {
        self.record.active_sessions
    }

    async fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.record.updated_at
    }

    #[graphql(deprecation = "Legacy JSON settings. Prefer typed fields over time.")]
    async fn settings(&self) -> JsonValue {
        JsonValue(self.record.settings.clone())
    }

    /// Optional default actor explicitly assigned to this port.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { assignedActor { id name status } } }
    /// ```
    async fn assigned_actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let Some(actor_id) = self.record.assigned_actor_id.as_ref() else {
            return Ok(None);
        };
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(actor_id)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }

    /// Session binding rows for this port.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { bindings(first: 10) { edges { node { conversationKey sessionId } } } } }
    /// ```
    async fn bindings(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PortBindingConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let bindings = data
            .db
            .list_port_bindings(&self.record.port_name, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(bindings, start, first);

        let edges = page
            .into_iter()
            .map(|(index, (conversation_key, session_id))| PortBindingEdge {
                cursor: encode_offset_cursor(index),
                node: PortBindingObject {
                    port_name: self.record.port_name.clone(),
                    conversation_key,
                    session_id,
                },
            })
            .collect::<Vec<_>>();

        Ok(PortBindingConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Actor binding rows for this port.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { actorBindings(first: 10) { edges { node { conversationKey actorId } } } } }
    /// ```
    async fn actor_bindings(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PortActorBindingConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let bindings = data
            .db
            .list_port_actor_bindings(&self.record.port_name, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(bindings, start, first);

        let edges = page
            .into_iter()
            .map(
                |(index, (conversation_key, actor_id))| PortActorBindingEdge {
                    cursor: encode_offset_cursor(index),
                    node: PortActorBindingObject {
                        port_name: self.record.port_name.clone(),
                        conversation_key,
                        actor_id,
                    },
                },
            )
            .collect::<Vec<_>>();

        Ok(PortActorBindingConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `PortBinding` object mapping `conversationKey -> sessionId`.
///
/// Usage notes:
/// - Canonical ingress-session routing row.
/// - `actor` resolves optional per-conversation actor override.
///
/// Example:
/// ```graphql
/// { port(name: "telegram") { bindings(first: 5) { edges { node { conversationKey sessionId actor { id } } } } } }
/// ```
struct PortBindingObject {
    port_name: String,
    conversation_key: Uri,
    session_id: Uri,
}

#[Object(name = "PortBinding", use_type_description)]
impl PortBindingObject {
    async fn id(&self) -> String {
        format!("{}:{}", self.port_name, self.conversation_key)
    }

    async fn port_name(&self) -> &str {
        &self.port_name
    }

    async fn conversation_key(&self) -> UriScalar {
        self.conversation_key.clone().into()
    }

    async fn session_id(&self) -> UriScalar {
        self.session_id.clone().into()
    }

    /// Resolved session object for this binding.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { bindings(first: 1) { edges { node { session { id updatedAt } } } } } }
    /// ```
    async fn session(&self, ctx: &Context<'_>) -> GqlResult<Option<SessionObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_session(&self.session_id)
            .await
            .map_err(map_anyhow)?
            .map(SessionObject::new))
    }

    /// Actor bound to this conversation key, if any.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { bindings(first: 1) { edges { node { actor { id name } } } } } }
    /// ```
    async fn actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        let actor = data
            .db
            .get_port_actor_binding(&self.port_name, &self.conversation_key)
            .await
            .map_err(map_anyhow)?;

        let Some(actor_id) = actor else {
            return Ok(None);
        };

        Ok(data
            .db
            .get_actor(&actor_id)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }
}

#[derive(Clone, Description)]
/// GraphQL `PortActorBinding` object mapping `conversationKey -> actorId`.
///
/// Usage notes:
/// - Stores actor override independent of session binding.
/// - `actorId = null` means no override exists.
///
/// Example:
/// ```graphql
/// { port(name: "telegram") { actorBindings(first: 5) { edges { node { conversationKey actorId } } } } }
/// ```
struct PortActorBindingObject {
    port_name: String,
    conversation_key: Uri,
    actor_id: Option<Uri>,
}

#[Object(name = "PortActorBinding", use_type_description)]
impl PortActorBindingObject {
    async fn id(&self) -> String {
        format!("{}:{}", self.port_name, self.conversation_key)
    }

    async fn port_name(&self) -> &str {
        &self.port_name
    }

    async fn conversation_key(&self) -> UriScalar {
        self.conversation_key.clone().into()
    }

    async fn actor_id(&self) -> Option<UriScalar> {
        self.actor_id.clone().map(UriScalar)
    }

    /// Expanded actor object for this actor binding row.
    async fn actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let Some(actor_id) = self.actor_id.as_ref() else {
            return Ok(None);
        };
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(actor_id)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }
}

#[derive(Clone, Description)]
/// GraphQL `Provider` object for LLM provider configuration/usage.
///
/// Usage notes:
/// - `provider` is the configuration key.
/// - `providerKind` maps to adapter family when needed.
///
/// Example:
/// ```graphql
/// { provider(provider: "openai") { provider providerKind enabled defaultTextModel } }
/// ```
struct ProviderObject {
    id: Uri,
    record: ProviderRecord,
}

impl ProviderObject {
    fn try_new(record: ProviderRecord) -> GqlResult<Self> {
        Ok(Self {
            id: provider_uri(&record.provider)?,
            record,
        })
    }
}

#[Object(name = "Provider", use_type_description)]
impl ProviderObject {
    async fn id(&self) -> UriScalar {
        self.id.clone().into()
    }

    async fn provider(&self) -> &str {
        &self.record.provider
    }

    async fn provider_kind(&self) -> &str {
        &self.record.provider_kind
    }

    async fn api_key(&self) -> &str {
        &self.record.api_key
    }

    async fn base_url(&self) -> Option<&str> {
        self.record.base_url.as_deref()
    }

    async fn enabled(&self) -> bool {
        self.record.enabled
    }

    async fn tokens_used(&self) -> u64 {
        self.record.tokens_used
    }

    async fn last_used(&self) -> Option<DateTime<Utc>> {
        self.record.last_used
    }

    async fn default_text_model(&self) -> Option<&str> {
        self.record.default_text_model.as_deref()
    }

    async fn default_audio_model(&self) -> Option<&str> {
        self.record.default_audio_model.as_deref()
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `App` object for capability/auth integrations.
///
/// Usage notes:
/// - Parent object for capabilities, external connections, and secrets.
/// - `authConfig` is transitional JSON; prefer typed auth fields as they are added.
///
/// Example:
/// ```graphql
/// { appBySlug(slug: "github") { id slug capabilities(first: 5) { edges { node { name mode } } } } }
/// ```
struct AppObject {
    record: AppRecord,
}

impl AppObject {
    fn new(record: AppRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "App", use_type_description)]
impl AppObject {
    async fn id(&self) -> UriScalar {
        self.record.app_id.clone().into()
    }

    async fn name(&self) -> &str {
        &self.record.name
    }

    async fn slug(&self) -> &str {
        &self.record.slug
    }

    async fn description(&self) -> &str {
        &self.record.description
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn built_in(&self) -> bool {
        self.record.built_in
    }

    async fn source(&self) -> &str {
        &self.record.source
    }

    async fn auth_strategy(&self) -> &str {
        &self.record.auth_strategy
    }

    async fn available_secrets(&self) -> Vec<String> {
        self.record.available_secrets.clone()
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    #[graphql(deprecation = "Legacy JSON auth config. Prefer typed auth fields over time.")]
    async fn auth_config(&self) -> JsonValue {
        JsonValue(self.record.auth_config_json.clone())
    }

    /// Capability definitions available on this app.
    ///
    /// Example:
    /// ```graphql
    /// { appBySlug(slug: "github") { capabilities(first: 20) { edges { node { id name mode } } } } }
    /// ```
    async fn capabilities(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<AppCapabilityConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let capabilities = data
            .db
            .list_app_capabilities(&self.record.app_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(capabilities, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| AppCapabilityEdge {
                cursor: encode_offset_cursor(index),
                node: AppCapabilityObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(AppCapabilityConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// External account connections for this app.
    ///
    /// Example:
    /// ```graphql
    /// { appBySlug(slug: "github") { connections(first: 20) { edges { node { id status ownerUserId } } } } }
    /// ```
    async fn connections(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<AppExternalConnectionConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let connections = data
            .db
            .list_app_connections(&self.record.app_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(connections, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| AppExternalConnectionEdge {
                cursor: encode_offset_cursor(index),
                node: AppExternalConnectionObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(AppExternalConnectionConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Secrets available to this app (optionally filtered by connection).
    ///
    /// Example:
    /// ```graphql
    /// { appBySlug(slug: "github") { secrets(first: 20) { edges { node { id key kind } } } } }
    /// ```
    async fn secrets(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        connection_id: Option<UriScalar>,
    ) -> GqlResult<AppSecretConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let secrets = data
            .db
            .list_app_secrets(
                &self.record.app_id,
                connection_id.as_ref().map(|uri| &uri.0),
                fetch_limit,
            )
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(secrets, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| AppSecretEdge {
                cursor: encode_offset_cursor(index),
                node: AppSecretObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(AppSecretConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `AppCapability` object linked to an app.
///
/// Usage notes:
/// - Capability rows describe app operations exposed to runtime/LLMs.
///
/// Example:
/// ```graphql
/// { appBySlug(slug: "github") { capabilities(first: 5) { edges { node { id name mode status } } } } }
/// ```
struct AppCapabilityObject {
    record: AppCapabilityRecord,
}

impl AppCapabilityObject {
    fn new(record: AppCapabilityRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppCapability", use_type_description)]
impl AppCapabilityObject {
    async fn id(&self) -> UriScalar {
        self.record.capability_id.clone().into()
    }

    async fn app_id(&self) -> UriScalar {
        self.record.app_id.clone().into()
    }

    async fn name(&self) -> &str {
        &self.record.name
    }

    async fn hint(&self) -> &str {
        &self.record.hint
    }

    async fn mode(&self) -> &str {
        &self.record.mode
    }

    async fn instructions(&self) -> &str {
        &self.record.instructions
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `AppConnection` object representing an external account connection.
///
/// Usage notes:
/// - Represents one user/account connection to an app integration.
/// - `connection` is transitional JSON metadata.
///
/// Example:
/// ```graphql
/// { appBySlug(slug: "github") { connections(first: 5) { edges { node { id ownerUserId status } } } } }
/// ```
struct AppExternalConnectionObject {
    record: AppConnectionRecord,
}

impl AppExternalConnectionObject {
    fn new(record: AppConnectionRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppConnection", use_type_description)]
impl AppExternalConnectionObject {
    async fn id(&self) -> UriScalar {
        self.record.connection_id.clone().into()
    }

    async fn app_id(&self) -> UriScalar {
        self.record.app_id.clone().into()
    }

    async fn owner_user_id(&self) -> Option<UriScalar> {
        self.record.owner_user_id.clone().map(UriScalar)
    }

    async fn provider_account_id(&self) -> Option<&str> {
        self.record.provider_account_id.as_deref()
    }

    async fn external_user_id(&self) -> Option<&str> {
        self.record.external_user_id.as_deref()
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    #[graphql(deprecation = "Legacy JSON connection payload. Prefer typed fields over time.")]
    async fn connection(&self) -> JsonValue {
        JsonValue(self.record.connection_json.clone())
    }
}

#[derive(Clone, Description)]
/// GraphQL `AppSecret` object for scoped secret material.
///
/// Usage notes:
/// - Secrets can be global per-app or scoped to `connectionId`.
///
/// Example:
/// ```graphql
/// { appBySlug(slug: "github") { secrets(first: 5) { edges { node { id key kind connectionId } } } } }
/// ```
struct AppSecretObject {
    record: AppSecretRecord,
}

impl AppSecretObject {
    fn new(record: AppSecretRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppSecret", use_type_description)]
impl AppSecretObject {
    async fn id(&self) -> UriScalar {
        self.record.secret_id.clone().into()
    }

    async fn app_id(&self) -> UriScalar {
        self.record.app_id.clone().into()
    }

    async fn connection_id(&self) -> Option<UriScalar> {
        self.record.connection_id.clone().map(UriScalar)
    }

    async fn key(&self) -> &str {
        &self.record.key
    }

    async fn value(&self) -> &str {
        &self.record.value
    }

    async fn kind(&self) -> &str {
        &self.record.kind
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `ClockworkJob` object for scheduler plans.
///
/// Usage notes:
/// - Defines recurring/queued actor execution plans.
/// - Use `runs` to inspect execution history.
///
/// Example:
/// ```graphql
/// { clockworkJob(jobId: "daily-digest") { id kind status nextRunAt runs(first: 5) { edges { node { id firedAt } } } } }
/// ```
struct ClockworkJobObject {
    record: ClockworkJobRecord,
}

impl ClockworkJobObject {
    fn new(record: ClockworkJobRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ClockworkJob", use_type_description)]
impl ClockworkJobObject {
    async fn id(&self) -> &str {
        &self.record.job_id
    }

    async fn kind(&self) -> &str {
        &self.record.kind
    }

    async fn status(&self) -> &str {
        &self.record.status
    }

    async fn target_actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.target_actor_id).ok().map(UriScalar)
    }

    async fn target_session_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.target_session_id)
            .ok()
            .map(UriScalar)
    }

    async fn message_type(&self) -> &str {
        &self.record.message_type
    }

    async fn next_run_at(&self) -> Option<DateTime<Utc>> {
        self.record.next_run_at
    }

    async fn last_run_at(&self) -> Option<DateTime<Utc>> {
        self.record.last_run_at
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    #[graphql(deprecation = "Legacy JSON payload. Prefer typed fields over time.")]
    async fn payload(&self) -> JsonValue {
        JsonValue(self.record.payload.clone())
    }

    #[graphql(deprecation = "Legacy JSON headers. Prefer typed fields over time.")]
    async fn headers(&self) -> JsonValue {
        JsonValue(self.record.headers.clone())
    }

    #[graphql(deprecation = "Legacy JSON schedule spec. Prefer typed schedule fields over time.")]
    async fn schedule_spec(&self) -> JsonValue {
        JsonValue(self.record.schedule_spec.clone())
    }

    /// Historical run rows for this clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// { clockworkJob(jobId: "daily") { runs(first: 10) { edges { node { id firedAt messageId } } } } }
    /// ```
    async fn runs(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ClockworkJobRunConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let runs = data
            .db
            .list_clockwork_job_runs(&self.record.job_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(runs, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ClockworkJobRunEdge {
                cursor: encode_offset_cursor(index),
                node: ClockworkJobRunObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ClockworkJobRunConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `ClockworkJobRun` object for executed scheduler runs.
///
/// Usage notes:
/// - Immutable execution row emitted by clockwork runtime.
///
/// Example:
/// ```graphql
/// { clockworkJob(jobId: "daily-digest") { runs(first: 5) { edges { node { id scheduledFor firedAt messageId } } } } }
/// ```
struct ClockworkJobRunObject {
    record: ClockworkJobRunRecord,
}

impl ClockworkJobRunObject {
    fn new(record: ClockworkJobRunRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ClockworkJobRun", use_type_description)]
impl ClockworkJobRunObject {
    async fn id(&self) -> &str {
        &self.record.run_id
    }

    async fn job_id(&self) -> &str {
        &self.record.job_id
    }

    async fn scheduled_for(&self) -> DateTime<Utc> {
        self.record.scheduled_for
    }

    async fn fired_at(&self) -> DateTime<Utc> {
        self.record.fired_at
    }

    async fn target_actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.target_actor_id).ok().map(UriScalar)
    }

    async fn target_session_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.target_session_id)
            .ok()
            .map(UriScalar)
    }

    async fn message_id(&self) -> &str {
        &self.record.message_id
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `Task` object from TaskGraph.
///
/// Usage notes:
/// - Task is the core work item in durable taskgraph storage.
/// - `parent`/`children` and `comments`/`events` provide graph + audit traversal.
///
/// Example:
/// ```graphql
/// { task(id: "borg:task:t1") { id title status children(first: 5) { edges { node { id title } } } } }
/// ```
struct TaskObject {
    id: Uri,
    record: TaskRecord,
}

impl TaskObject {
    fn try_new(record: TaskRecord) -> GqlResult<Self> {
        let id = parse_core_uri(&record.uri)?;
        Ok(Self { id, record })
    }
}

#[Object(name = "Task", use_type_description)]
impl TaskObject {
    async fn id(&self) -> UriScalar {
        self.id.clone().into()
    }

    async fn title(&self) -> &str {
        &self.record.title
    }

    async fn description(&self) -> &str {
        &self.record.description
    }

    async fn definition_of_done(&self) -> &str {
        &self.record.definition_of_done
    }

    async fn status(&self) -> TaskStatusValue {
        TaskStatusValue::from_raw(&self.record.status)
    }

    async fn assignee_agent_id(&self) -> &str {
        &self.record.assignee_agent_id
    }

    async fn assignee_session_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.assignee_session_uri)
            .ok()
            .map(UriScalar)
    }

    async fn reviewer_agent_id(&self) -> &str {
        &self.record.reviewer_agent_id
    }

    async fn reviewer_session_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.reviewer_session_uri)
            .ok()
            .map(UriScalar)
    }

    async fn labels(&self) -> Vec<String> {
        self.record.labels.clone()
    }

    async fn parent_uri(&self) -> Option<UriScalar> {
        self.record
            .parent_uri
            .as_deref()
            .and_then(|raw| Uri::parse(raw).ok())
            .map(UriScalar)
    }

    async fn blocked_by(&self) -> Vec<UriScalar> {
        self.record
            .blocked_by
            .iter()
            .filter_map(|raw| Uri::parse(raw).ok())
            .map(UriScalar)
            .collect::<Vec<_>>()
    }

    async fn duplicate_of(&self) -> Option<UriScalar> {
        self.record
            .duplicate_of
            .as_deref()
            .and_then(|raw| Uri::parse(raw).ok())
            .map(UriScalar)
    }

    async fn references(&self) -> Vec<UriScalar> {
        self.record
            .references
            .iter()
            .filter_map(|raw| Uri::parse(raw).ok())
            .map(UriScalar)
            .collect::<Vec<_>>()
    }

    async fn created_at(&self) -> String {
        self.record.created_at.clone()
    }

    async fn updated_at(&self) -> String {
        self.record.updated_at.clone()
    }

    /// Review state timestamps for this task.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t1") { review { submittedAt approvedAt changesRequestedAt } } }
    /// ```
    async fn review(&self) -> ReviewStateObject {
        ReviewStateObject {
            submitted_at: self.record.review.submitted_at.clone(),
            approved_at: self.record.review.approved_at.clone(),
            changes_requested_at: self.record.review.changes_requested_at.clone(),
        }
    }

    /// Parent task object, if this task is a subtask.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t-child") { parent { id title status } } }
    /// ```
    async fn parent(&self, ctx: &Context<'_>) -> GqlResult<Option<TaskObject>> {
        let Some(parent_uri) = self.record.parent_uri.as_deref() else {
            return Ok(None);
        };
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        match store.get_task(parent_uri).await {
            Ok(task) => Ok(Some(TaskObject::try_new(task)?)),
            Err(err) if err.to_string().contains("not_found") => Ok(None),
            Err(err) => Err(map_anyhow(err)),
        }
    }

    /// Child task rows directly under this task.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t-parent") { children(first: 20) { edges { node { id title status } } } } }
    /// ```
    async fn children(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<TaskConnection> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let first = data.normalize_first(first)?;

        let (tasks, next_cursor) = store
            .list_task_children(
                self.id.as_str(),
                ListParams {
                    cursor: after,
                    limit: first,
                },
            )
            .await
            .map_err(map_anyhow)?;

        let mut edges = Vec::with_capacity(tasks.len());
        for task in tasks {
            let cursor = encode_task_cursor(&task.created_at, &task.uri);
            edges.push(TaskEdge {
                cursor,
                node: TaskObject::try_new(task)?,
            });
        }

        Ok(TaskConnection {
            page_info: PageInfo {
                has_next_page: next_cursor.is_some(),
                end_cursor: edges.last().map(|edge| edge.cursor.clone()).or(next_cursor),
            },
            edges,
        })
    }

    /// Comment timeline for this task.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t1") { comments(first: 20) { edges { node { id body authorSessionUri } } } } }
    /// ```
    async fn comments(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<TaskCommentConnection> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let first = data.normalize_first(first)?;

        let (comments, next_cursor) = store
            .list_comments(
                self.id.as_str(),
                ListParams {
                    cursor: after,
                    limit: first,
                },
            )
            .await
            .map_err(map_anyhow)?;

        let edges = comments
            .into_iter()
            .map(|comment| {
                let cursor = encode_task_cursor(&comment.created_at, &comment.id);
                TaskCommentEdge {
                    cursor,
                    node: TaskCommentObject::new(comment),
                }
            })
            .collect::<Vec<_>>();

        Ok(TaskCommentConnection {
            page_info: PageInfo {
                has_next_page: next_cursor.is_some(),
                end_cursor: edges.last().map(|edge| edge.cursor.clone()).or(next_cursor),
            },
            edges,
        })
    }

    /// Event timeline for this task.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t1") { events(first: 20) { edges { node { id type createdAt } } } } }
    /// ```
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<TaskEventConnection> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let first = data.normalize_first(first)?;

        let (events, next_cursor) = store
            .list_events(
                self.id.as_str(),
                ListParams {
                    cursor: after,
                    limit: first,
                },
            )
            .await
            .map_err(map_anyhow)?;

        let edges = events
            .into_iter()
            .map(|event| {
                let cursor = encode_task_cursor(&event.created_at, &event.id);
                TaskEventEdge {
                    cursor,
                    node: TaskEventObject::new(event),
                }
            })
            .collect::<Vec<_>>();

        Ok(TaskEventConnection {
            page_info: PageInfo {
                has_next_page: next_cursor.is_some(),
                end_cursor: edges.last().map(|edge| edge.cursor.clone()).or(next_cursor),
            },
            edges,
        })
    }
}

#[derive(SimpleObject, Clone)]
/// Task review lifecycle timestamps.
///
/// Usage notes:
/// - A field is `null` until that transition happened.
struct ReviewStateObject {
    /// Timestamp when task entered review.
    submitted_at: Option<String>,
    /// Timestamp when task was approved.
    approved_at: Option<String>,
    /// Timestamp when changes were requested.
    changes_requested_at: Option<String>,
}

#[derive(Clone, Description)]
/// GraphQL `TaskComment` object.
///
/// Usage notes:
/// - Comment timeline entries attached to a task.
///
/// Example:
/// ```graphql
/// { task(id: "borg:task:t1") { comments(first: 5) { edges { node { id body createdAt } } } } }
/// ```
struct TaskCommentObject {
    record: borg_taskgraph::CommentRecord,
}

impl TaskCommentObject {
    fn new(record: borg_taskgraph::CommentRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "TaskComment", use_type_description)]
impl TaskCommentObject {
    /// Stable comment identifier.
    async fn id(&self) -> &str {
        &self.record.id
    }

    /// Task URI that this comment belongs to.
    async fn task_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.task_uri).ok().map(UriScalar)
    }

    /// Session URI that authored this comment.
    async fn author_session_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.author_session_uri)
            .ok()
            .map(UriScalar)
    }

    /// Comment body text.
    async fn body(&self) -> &str {
        &self.record.body
    }

    /// Comment creation timestamp.
    async fn created_at(&self) -> &str {
        &self.record.created_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `TaskEvent` object.
///
/// Usage notes:
/// - Event timeline entries with typed payload projection in `data`.
///
/// Example:
/// ```graphql
/// { task(id: "borg:task:t1") { events(first: 5) { edges { node { id type data { kind status } } } } } }
/// ```
struct TaskEventObject {
    record: EventRecord,
}

impl TaskEventObject {
    fn new(record: EventRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "TaskEvent", use_type_description)]
impl TaskEventObject {
    /// Stable event identifier.
    async fn id(&self) -> &str {
        &self.record.id
    }

    /// Task URI that emitted this event.
    async fn task_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.task_uri).ok().map(UriScalar)
    }

    /// Session URI that triggered the event.
    async fn actor_session_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.actor_session_uri)
            .ok()
            .map(UriScalar)
    }

    #[graphql(name = "type")]
    async fn event_type(&self) -> &str {
        &self.record.event_type
    }

    /// Event payload projected into typed optional fields.
    ///
    /// Usage notes:
    /// - Read `kind` first, then use matching payload fields.
    ///
    /// Example:
    /// ```graphql
    /// { task(id: "borg:task:t1") { events(first: 1) { edges { node { type data { kind status note } } } } } }
    /// ```
    async fn data(&self) -> GqlResult<TaskEventDataObject> {
        let value =
            serde_json::to_value(&self.record.data).map_err(|err| map_anyhow(err.into()))?;
        Ok(TaskEventDataObject::from_json(
            self.record.event_type.clone(),
            value,
        ))
    }

    /// Event creation timestamp.
    async fn created_at(&self) -> &str {
        &self.record.created_at
    }
}

#[derive(SimpleObject, Clone)]
/// Typed projection of task event payload data.
///
/// Usage notes:
/// - `kind` indicates which subset of optional fields is populated.
/// - Consumers should branch on `kind` before reading event-specific fields.
struct TaskEventDataObject {
    /// Event type/kind copied from the event row.
    kind: String,
    /// New assignee actor ID when relevant.
    assignee_agent_id: Option<String>,
    /// New assignee session URI when relevant.
    assignee_session_uri: Option<String>,
    /// Reviewer actor ID when relevant.
    reviewer_agent_id: Option<String>,
    /// Reviewer session URI when relevant.
    reviewer_session_uri: Option<String>,
    /// Parent task URI when relevant.
    parent_uri: Option<String>,
    /// Updated title when relevant.
    title: Option<String>,
    /// Updated description when relevant.
    description: Option<String>,
    /// Updated definition-of-done when relevant.
    definition_of_done: Option<String>,
    /// Previous assignee actor ID when relevant.
    old_assignee_agent_id: Option<String>,
    /// Previous assignee session URI when relevant.
    old_assignee_session_uri: Option<String>,
    /// Replacement assignee actor ID when relevant.
    new_assignee_agent_id: Option<String>,
    /// Replacement assignee session URI when relevant.
    new_assignee_session_uri: Option<String>,
    /// Label list payload when relevant.
    labels: Option<Vec<String>>,
    /// Blocking dependency URI when relevant.
    blocked_by: Option<String>,
    /// Duplicate-of URI when relevant.
    duplicate_of: Option<String>,
    /// Reference URI when relevant.
    reference: Option<String>,
    /// Status value when relevant.
    status: Option<String>,
    /// Review submit timestamp when relevant.
    submitted_at: Option<String>,
    /// Review approval timestamp when relevant.
    approved_at: Option<String>,
    /// Review changes-requested timestamp when relevant.
    changes_requested_at: Option<String>,
    /// Return status destination when relevant.
    return_to: Option<String>,
    /// Free-text note when relevant.
    note: Option<String>,
    /// Subtask count delta when relevant.
    subtask_count: Option<i64>,
    /// Created comment ID when relevant.
    comment_id: Option<String>,
}

#[derive(Default, Deserialize)]
struct TaskEventDataSerde {
    #[serde(default)]
    assignee_agent_id: Option<String>,
    #[serde(default)]
    assignee_session_uri: Option<String>,
    #[serde(default)]
    reviewer_agent_id: Option<String>,
    #[serde(default)]
    reviewer_session_uri: Option<String>,
    #[serde(default)]
    parent_uri: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
    #[serde(default)]
    old_assignee_agent_id: Option<String>,
    #[serde(default)]
    old_assignee_session_uri: Option<String>,
    #[serde(default)]
    new_assignee_agent_id: Option<String>,
    #[serde(default)]
    new_assignee_session_uri: Option<String>,
    #[serde(default)]
    labels: Option<Vec<String>>,
    #[serde(default)]
    blocked_by: Option<String>,
    #[serde(default)]
    duplicate_of: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    submitted_at: Option<String>,
    #[serde(default)]
    approved_at: Option<String>,
    #[serde(default)]
    changes_requested_at: Option<String>,
    #[serde(default)]
    return_to: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    subtask_count: Option<i64>,
    #[serde(default)]
    comment_id: Option<String>,
}

impl TaskEventDataObject {
    fn from_json(kind: String, value: serde_json::Value) -> Self {
        let parsed = serde_json::from_value::<TaskEventDataSerde>(value).unwrap_or_default();
        Self {
            kind,
            assignee_agent_id: parsed.assignee_agent_id,
            assignee_session_uri: parsed.assignee_session_uri,
            reviewer_agent_id: parsed.reviewer_agent_id,
            reviewer_session_uri: parsed.reviewer_session_uri,
            parent_uri: parsed.parent_uri,
            title: parsed.title,
            description: parsed.description,
            definition_of_done: parsed.definition_of_done,
            old_assignee_agent_id: parsed.old_assignee_agent_id,
            old_assignee_session_uri: parsed.old_assignee_session_uri,
            new_assignee_agent_id: parsed.new_assignee_agent_id,
            new_assignee_session_uri: parsed.new_assignee_session_uri,
            labels: parsed.labels,
            blocked_by: parsed.blocked_by,
            duplicate_of: parsed.duplicate_of,
            reference: parsed.reference,
            status: parsed.status,
            submitted_at: parsed.submitted_at,
            approved_at: parsed.approved_at,
            changes_requested_at: parsed.changes_requested_at,
            return_to: parsed.return_to,
            note: parsed.note,
            subtask_count: parsed.subtask_count,
            comment_id: parsed.comment_id,
        }
    }
}

#[derive(Clone, Description)]
/// GraphQL `MemoryEntity` object from the memory graph.
///
/// Usage notes:
/// - Entity vertex with typed property map (`props`).
/// - `facts` exposes fact rows that target this entity.
///
/// Example:
/// ```graphql
/// { memoryEntity(id: "borg:entity:alice") { id label props { key value { kind text } } } }
/// ```
struct MemoryEntityObject {
    record: Entity,
}

impl MemoryEntityObject {
    fn new(record: Entity) -> Self {
        Self { record }
    }
}

#[Object(name = "MemoryEntity", use_type_description)]
impl MemoryEntityObject {
    async fn id(&self) -> UriScalar {
        self.record.entity_id.clone().into()
    }

    async fn entity_type(&self) -> UriScalar {
        self.record.entity_type.clone().into()
    }

    async fn label(&self) -> &str {
        &self.record.label
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    /// Property map on the entity, projected as typed key/value pairs.
    ///
    /// Example:
    /// ```graphql
    /// { memoryEntity(id: "borg:entity:alice") { props { key value { kind text reference } } } }
    /// ```
    async fn props(&self) -> Vec<MemoryPropertyObject> {
        self.record
            .props
            .iter()
            .map(|(key, value)| MemoryPropertyObject {
                key: key.clone(),
                value: MemoryValueObject::from_entity_value(value),
            })
            .collect::<Vec<_>>()
    }

    /// Facts that target this memory entity.
    ///
    /// Example:
    /// ```graphql
    /// { memoryEntity(id: "borg:entity:alice") { facts(first: 20) { edges { node { field value { kind text } } } } } }
    /// ```
    async fn facts(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        field_id: Option<UriScalar>,
        include_retracted: Option<bool>,
    ) -> GqlResult<MemoryFactConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let entity_id = to_memory_uri(&self.record.entity_id)?;
        let field = field_id
            .as_ref()
            .map(|uri| to_memory_uri(&uri.0))
            .transpose()?;

        let facts = data
            .memory
            .list_facts(
                Some(&entity_id),
                field.as_ref(),
                include_retracted.unwrap_or(false),
                fetch_limit,
            )
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(facts, start, first);
        let edges = page
            .into_iter()
            .map(|(index, record)| MemoryFactEdge {
                cursor: encode_offset_cursor(index),
                node: MemoryFactObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(MemoryFactConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(SimpleObject, Clone)]
/// One key/value pair from `MemoryEntity.props`.
struct MemoryPropertyObject {
    /// Property key name.
    key: String,
    /// Typed property value.
    value: MemoryValueObject,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Discriminator for `MemoryValueObject`.
enum MemoryValueKind {
    Text,
    Integer,
    Float,
    Boolean,
    Bytes,
    Ref,
    Date,
    DateTime,
    List,
}

#[derive(SimpleObject, Clone)]
/// Typed value projection used by memory entities/facts.
///
/// Usage notes:
/// - Inspect `kind` first, then read the matching typed field.
/// - Non-matching fields remain `null`.
struct MemoryValueObject {
    /// Value discriminator.
    kind: MemoryValueKind,
    /// String value when `kind = TEXT`.
    text: Option<String>,
    /// Integer value when `kind = INTEGER`.
    integer: Option<i64>,
    /// Floating-point value when `kind = FLOAT`.
    float: Option<f64>,
    /// Boolean value when `kind = BOOLEAN`.
    boolean: Option<bool>,
    /// Base64-url encoded bytes when `kind = BYTES`.
    bytes_base64: Option<String>,
    /// URI reference when `kind = REF`.
    reference: Option<UriScalar>,
    /// Date value (`YYYY-MM-DD`) when `kind = DATE`.
    date: Option<String>,
    /// Date-time value (RFC3339) when `kind = DATE_TIME`.
    datetime: Option<String>,
    /// Nested typed values when `kind = LIST`.
    list: Option<Vec<MemoryValueObject>>,
}

impl MemoryValueObject {
    fn empty(kind: MemoryValueKind) -> Self {
        Self {
            kind,
            text: None,
            integer: None,
            float: None,
            boolean: None,
            bytes_base64: None,
            reference: None,
            date: None,
            datetime: None,
            list: None,
        }
    }

    fn from_entity_value(value: &EntityPropValue) -> Self {
        match value {
            EntityPropValue::Text(text) => Self {
                kind: MemoryValueKind::Text,
                text: Some(text.clone()),
                ..Self::empty(MemoryValueKind::Text)
            },
            EntityPropValue::Integer(integer) => Self {
                kind: MemoryValueKind::Integer,
                integer: Some(*integer),
                ..Self::empty(MemoryValueKind::Integer)
            },
            EntityPropValue::Float(float) => Self {
                kind: MemoryValueKind::Float,
                float: Some(*float),
                ..Self::empty(MemoryValueKind::Float)
            },
            EntityPropValue::Boolean(boolean) => Self {
                kind: MemoryValueKind::Boolean,
                boolean: Some(*boolean),
                ..Self::empty(MemoryValueKind::Boolean)
            },
            EntityPropValue::Bytes(bytes) => Self {
                kind: MemoryValueKind::Bytes,
                bytes_base64: Some(URL_SAFE_NO_PAD.encode(bytes)),
                ..Self::empty(MemoryValueKind::Bytes)
            },
            EntityPropValue::Ref(reference) => Self {
                kind: MemoryValueKind::Ref,
                reference: Some(reference.clone().into()),
                ..Self::empty(MemoryValueKind::Ref)
            },
            EntityPropValue::List(items) => Self {
                kind: MemoryValueKind::List,
                list: Some(items.iter().map(Self::from_entity_value).collect()),
                ..Self::empty(MemoryValueKind::List)
            },
        }
    }

    fn from_fact_value(value: &FactValue) -> Self {
        match value {
            FactValue::Text(text) => Self {
                kind: MemoryValueKind::Text,
                text: Some(text.clone()),
                ..Self::empty(MemoryValueKind::Text)
            },
            FactValue::Integer(integer) => Self {
                kind: MemoryValueKind::Integer,
                integer: Some(*integer),
                ..Self::empty(MemoryValueKind::Integer)
            },
            FactValue::Float(float) => Self {
                kind: MemoryValueKind::Float,
                float: Some(*float),
                ..Self::empty(MemoryValueKind::Float)
            },
            FactValue::Boolean(boolean) => Self {
                kind: MemoryValueKind::Boolean,
                boolean: Some(*boolean),
                ..Self::empty(MemoryValueKind::Boolean)
            },
            FactValue::Bytes(bytes) => Self {
                kind: MemoryValueKind::Bytes,
                bytes_base64: Some(URL_SAFE_NO_PAD.encode(bytes)),
                ..Self::empty(MemoryValueKind::Bytes)
            },
            FactValue::Ref(reference) => Self {
                kind: MemoryValueKind::Ref,
                reference: from_memory_uri(reference).ok(),
                ..Self::empty(MemoryValueKind::Ref)
            },
            FactValue::Date(date) => Self {
                kind: MemoryValueKind::Date,
                date: Some(date.clone()),
                ..Self::empty(MemoryValueKind::Date)
            },
            FactValue::DateTime(datetime) => Self {
                kind: MemoryValueKind::DateTime,
                datetime: Some(datetime.clone()),
                ..Self::empty(MemoryValueKind::DateTime)
            },
            FactValue::List(items) => Self {
                kind: MemoryValueKind::List,
                list: Some(items.iter().map(Self::from_fact_value).collect()),
                ..Self::empty(MemoryValueKind::List)
            },
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Cardinality of a memory fact field.
enum MemoryFactArity {
    One,
    Many,
}

impl From<FactArity> for MemoryFactArity {
    fn from(value: FactArity) -> Self {
        match value {
            FactArity::One => Self::One,
            FactArity::Many => Self::Many,
        }
    }
}

#[derive(Clone, Description)]
/// GraphQL `MemoryFact` object from the fact store.
///
/// Usage notes:
/// - Immutable fact rows with typed value projection.
/// - Use `isRetracted` for audit-aware consumers.
///
/// Example:
/// ```graphql
/// { memoryFacts(first: 5) { edges { node { id field arity value { kind text } isRetracted } } } }
/// ```
struct MemoryFactObject {
    record: FactRecord,
}

impl MemoryFactObject {
    fn new(record: FactRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "MemoryFact", use_type_description)]
impl MemoryFactObject {
    /// Fact URI.
    async fn id(&self) -> GqlResult<UriScalar> {
        from_memory_uri(&self.record.fact_id)
    }

    /// Source URI that asserted this fact.
    async fn source(&self) -> GqlResult<UriScalar> {
        from_memory_uri(&self.record.source)
    }

    /// Entity URI that this fact targets.
    async fn entity(&self) -> GqlResult<UriScalar> {
        from_memory_uri(&self.record.entity)
    }

    /// Field URI for this fact.
    async fn field(&self) -> GqlResult<UriScalar> {
        from_memory_uri(&self.record.field)
    }

    /// Fact field cardinality (`ONE` or `MANY`).
    async fn arity(&self) -> MemoryFactArity {
        self.record.arity.into()
    }

    /// Typed value projection for the fact payload.
    async fn value(&self) -> MemoryValueObject {
        MemoryValueObject::from_fact_value(&self.record.value)
    }

    /// Transaction URI that wrote this fact row.
    async fn tx_id(&self) -> GqlResult<UriScalar> {
        from_memory_uri(&self.record.tx_id)
    }

    /// Timestamp when the fact was stated.
    async fn stated_at(&self) -> DateTime<Utc> {
        self.record.stated_at
    }

    /// Whether the fact has been retracted.
    async fn is_retracted(&self) -> bool {
        self.record.is_retracted
    }
}

#[derive(Clone, Description)]
/// GraphQL `Policy` object.
///
/// Usage notes:
/// - Control-plane policy row.
/// - `uses` traverses entities policy is attached to.
///
/// Example:
/// ```graphql
/// { policies(first: 5) { edges { node { id uses(first: 5) { edges { node { entityId } } } } } } }
/// ```
struct PolicyObject {
    record: PolicyRecord,
}

impl PolicyObject {
    fn new(record: PolicyRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Policy", use_type_description)]
impl PolicyObject {
    async fn id(&self) -> UriScalar {
        self.record.policy_id.clone().into()
    }

    #[graphql(deprecation = "Legacy JSON policy payload. Prefer typed policy fields over time.")]
    async fn policy(&self) -> JsonValue {
        JsonValue(self.record.policy.clone())
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    /// Entities that this policy is attached to.
    ///
    /// Example:
    /// ```graphql
    /// { policy(id: "borg:policy:p1") { uses(first: 20) { edges { node { entityId createdAt } } } } }
    /// ```
    async fn uses(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PolicyUseConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let rows = data
            .db
            .list_policy_uses(&self.record.policy_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(rows, start, first);

        let edges = page
            .into_iter()
            .map(|(index, row)| PolicyUseEdge {
                cursor: encode_offset_cursor(index),
                node: PolicyUseObject::new(row),
            })
            .collect::<Vec<_>>();

        Ok(PolicyUseConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// GraphQL `PolicyUse` relationship object.
///
/// Example:
/// ```graphql
/// { policies(first: 1) { edges { node { uses(first: 5) { edges { node { policyId entityId } } } } } } }
/// ```
struct PolicyUseObject {
    record: PolicyUseRecord,
}

impl PolicyUseObject {
    fn new(record: PolicyUseRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "PolicyUse", use_type_description)]
impl PolicyUseObject {
    /// Policy URI.
    async fn policy_id(&self) -> UriScalar {
        self.record.policy_id.clone().into()
    }

    /// Target entity URI that policy applies to.
    async fn entity_id(&self) -> UriScalar {
        self.record.entity_id.clone().into()
    }

    /// Attachment timestamp.
    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }
}

#[derive(Clone, Description)]
/// GraphQL `User` object.
///
/// Usage notes:
/// - Minimal control-plane user row.
/// - `profile` is transitional JSON.
///
/// Example:
/// ```graphql
/// { users(first: 10) { edges { node { id createdAt updatedAt } } } }
/// ```
struct UserObject {
    record: UserRecord,
}

impl UserObject {
    fn new(record: UserRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "User", use_type_description)]
impl UserObject {
    async fn id(&self) -> UriScalar {
        self.record.user_key.clone().into()
    }

    #[graphql(deprecation = "Legacy JSON profile payload. Prefer typed profile fields over time.")]
    async fn profile(&self) -> JsonValue {
        JsonValue(self.record.profile.clone())
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Interface, Clone)]
#[graphql(field(name = "id", ty = "UriScalar"))]
/// Unified node interface for cross-entity graph traversal.
///
/// Example:
/// ```graphql
/// query($id: Uri!) {
///   node(id: $id) {
///     id
///     ... on Session { updatedAt }
///   }
/// }
/// ```
enum Node {
    Actor(ActorObject),
    Behavior(BehaviorObject),
    Session(SessionObject),
    Port(PortObject),
    Provider(ProviderObject),
    App(AppObject),
    Task(TaskObject),
    MemoryEntity(MemoryEntityObject),
    Policy(PolicyObject),
    User(UserObject),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// TaskGraph status values accepted by `setTaskStatus`.
enum TaskStatusValue {
    /// Newly created task.
    Pending,
    /// Work in progress.
    Doing,
    /// Awaiting review.
    Review,
    /// Completed successfully.
    Done,
    /// Explicitly discarded.
    Discarded,
}

impl TaskStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw {
            "pending" => Self::Pending,
            "doing" => Self::Doing,
            "review" => Self::Review,
            "done" => Self::Done,
            "discarded" => Self::Discarded,
            _ => Self::Pending,
        }
    }
}

impl From<TaskStatusValue> for TaskStatus {
    fn from(value: TaskStatusValue) -> Self {
        match value {
            TaskStatusValue::Pending => TaskStatus::Pending,
            TaskStatusValue::Doing => TaskStatus::Doing,
            TaskStatusValue::Review => TaskStatus::Review,
            TaskStatusValue::Done => TaskStatus::Done,
            TaskStatusValue::Discarded => TaskStatus::Discarded,
        }
    }
}

#[derive(SimpleObject)]
/// Placeholder response shape for `runActorChat`.
struct RunActorChatResult {
    /// Whether the runtime call succeeded.
    ok: bool,
    /// Human-readable status message.
    message: String,
}

#[derive(SimpleObject)]
/// Placeholder response shape for `runPortHttp`.
struct RunPortHttpResult {
    /// Whether the runtime call succeeded.
    ok: bool,
    /// Human-readable status message.
    message: String,
}

#[derive(Default)]
struct ParsedSessionMessage {
    message_type: String,
    role: Option<String>,
    text: Option<String>,
}

fn parse_session_message(payload: &serde_json::Value) -> ParsedSessionMessage {
    let Some(obj) = payload.as_object() else {
        return ParsedSessionMessage {
            message_type: "unknown".to_string(),
            role: None,
            text: None,
        };
    };

    let message_type = obj
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("legacy")
        .to_string();

    let role = if let Some(role) = obj.get("role").and_then(serde_json::Value::as_str) {
        Some(role.to_string())
    } else {
        match message_type.as_str() {
            "user" | "user_audio" => Some("user".to_string()),
            "assistant" => Some("assistant".to_string()),
            "system" => Some("system".to_string()),
            "tool_call" | "tool_result" => Some("tool".to_string()),
            "session_event" => Some("event".to_string()),
            _ => None,
        }
    };

    let text = obj
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            obj.get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            obj.get("transcript")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });

    ParsedSessionMessage {
        message_type,
        role,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::futures_util::StreamExt;

    fn tmp_path(prefix: &str, ext: &str) -> String {
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}-{}.{}", uuid::Uuid::new_v4(), ext));
        path.to_string_lossy().to_string()
    }

    async fn test_schema() -> anyhow::Result<BorgGqlSchema> {
        let db_path = tmp_path("borg-gql-test-db", "db");
        let memory_path = tmp_path("borg-gql-test-memory", "db");
        let search_path = tmp_path("borg-gql-test-search", "db");

        let db = BorgDb::open_local(&db_path).await?;
        db.migrate().await?;

        let memory = MemoryStore::new(&memory_path, &search_path)?;
        memory.migrate().await?;

        Ok(build_schema(db, memory))
    }

    #[tokio::test]
    async fn actor_workspace_query_roundtrip() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let data = schema.data::<BorgGqlData>().expect("gql data").clone();

        let behavior_id = Uri::from_parts("borg", "behavior", Some("default"))?;
        let actor_id = Uri::from_parts("borg", "actor", Some("a1"))?;
        let session_id = Uri::from_parts("borg", "session", Some("s1"))?;
        let user_id = Uri::from_parts("borg", "user", Some("u1"))?;
        let port_id = Uri::from_parts("borg", "port", Some("http"))?;

        data.db
            .upsert_behavior(
                &behavior_id,
                "default",
                "prompt",
                None,
                &json!(["search"]),
                "serial",
                "ACTIVE",
            )
            .await?;
        data.db
            .upsert_actor(&actor_id, "actor", "prompt", &behavior_id, "RUNNING")
            .await?;
        data.db
            .upsert_session(&session_id, &[user_id], &port_id)
            .await?;
        data.db
            .append_session_message(&session_id, &json!({"type":"user","content":"hello"}))
            .await?;
        data.db
            .enqueue_actor_message(
                &actor_id,
                "test",
                Some(&session_id),
                &json!({"source":"tests"}),
                None,
                None,
            )
            .await?;

        let query = r#"
          query($id: Uri!) {
            actor(id: $id) {
              id
              name
              defaultBehavior { id name }
              sessions(first: 5) {
                edges {
                  node {
                    id
                    messages(first: 5) {
                      edges {
                        node {
                          messageIndex
                          messageType
                          role
                          text
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        "#;

        let response = schema
            .execute(
                async_graphql::Request::new(query)
                    .variables(async_graphql::Variables::from_json(json!({"id": actor_id}))),
            )
            .await;

        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let data = response.data.into_json()?;
        assert_eq!(data["actor"]["name"], "actor");
        assert_eq!(
            data["actor"]["sessions"]["edges"][0]["node"]["messages"]["edges"][0]["node"]["text"],
            "hello"
        );

        Ok(())
    }

    #[tokio::test]
    async fn upsert_and_list_provider_via_graphql() -> anyhow::Result<()> {
        let schema = test_schema().await?;

        let mutation = r#"
          mutation {
            upsertProvider(input: {
              provider: "openai"
              providerKind: "openai"
              apiKey: "sk-test"
              enabled: true
              defaultTextModel: "gpt-4.1-mini"
            }) {
              provider
              providerKind
              enabled
              defaultTextModel
            }
          }
        "#;

        let response = schema.execute(mutation).await;
        assert!(response.errors.is_empty(), "{:#?}", response.errors);

        let query = r#"
          query {
            providers(first: 10) {
              edges {
                node {
                  provider
                  providerKind
                }
              }
            }
          }
        "#;
        let response = schema.execute(query).await;
        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let data = response.data.into_json()?;
        assert_eq!(data["providers"]["edges"][0]["node"]["provider"], "openai");

        Ok(())
    }

    #[tokio::test]
    async fn task_creation_and_status_transition() -> anyhow::Result<()> {
        let schema = test_schema().await?;

        let session_uri = Uri::from_parts("borg", "session", Some("task-session"))?;
        let creator = Uri::from_parts("borg", "actor", Some("creator"))?;
        let assignee = Uri::from_parts("borg", "actor", Some("assignee"))?;

        let create_mutation = r#"
          mutation CreateTask($session: Uri!, $creator: Uri!, $assignee: Uri!) {
            createTask(input: {
              sessionUri: $session
              creatorAgentId: $creator
              assigneeAgentId: $assignee
              title: "Ship borg-gql"
              description: "Implement gql"
              definitionOfDone: "tests pass"
            }) {
              id
              title
              status
              assigneeSessionId
            }
          }
        "#;

        let create_response = schema
            .execute(async_graphql::Request::new(create_mutation).variables(
                async_graphql::Variables::from_json(json!({
                    "session": session_uri,
                    "creator": creator,
                    "assignee": assignee,
                })),
            ))
            .await;

        assert!(
            create_response.errors.is_empty(),
            "{:#?}",
            create_response.errors
        );

        let created = create_response.data.into_json()?;
        let created_id = created["createTask"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing task id"))?
            .to_string();
        let assignee_session = created["createTask"]["assigneeSessionId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing assignee session"))?
            .to_string();

        let status_mutation = r#"
          mutation SetTask($task: Uri!, $session: Uri!) {
            setTaskStatus(input: {
              taskId: $task
              sessionUri: $session
              status: DOING
            }) {
              id
              status
            }
          }
        "#;

        let status_response = schema
            .execute(async_graphql::Request::new(status_mutation).variables(
                async_graphql::Variables::from_json(json!({
                    "task": created_id,
                    "session": assignee_session,
                })),
            ))
            .await;

        assert!(
            status_response.errors.is_empty(),
            "{:#?}",
            status_response.errors
        );

        let status = status_response.data.into_json()?;
        assert_eq!(status["setTaskStatus"]["status"], "DOING");
        Ok(())
    }

    #[tokio::test]
    async fn memory_entities_and_facts_are_typed() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let data = schema.data::<BorgGqlData>().expect("gql data").clone();

        let source = MemoryUri::from_parts("borg", "source", Some("tests"))?;
        let entity = MemoryUri::from_parts("borg", "entity", Some("alice"))?;
        let field = MemoryUri::from_parts("borg", "field", Some("name"))?;

        data.memory
            .state_facts(vec![borg_memory::FactInput {
                source,
                entity,
                field,
                arity: FactArity::One,
                value: FactValue::Text("Alice".to_string()),
            }])
            .await?;

        let query = r#"
          query {
            memoryFacts(first: 10) {
              edges {
                node {
                  arity
                  value { kind text }
                }
              }
            }
          }
        "#;

        let response = schema.execute(query).await;
        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let data = response.data.into_json()?;
        assert_eq!(
            data["memoryFacts"]["edges"][0]["node"]["value"]["text"],
            "Alice"
        );

        Ok(())
    }

    #[tokio::test]
    async fn schema_has_node_interface_and_core_types() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let sdl = schema.sdl();
        assert!(sdl.contains("interface Node"));
        assert!(sdl.contains("type Actor"));
        assert!(sdl.contains("type Session"));
        assert!(sdl.contains("type Task"));
        assert!(sdl.contains("scalar Uri"));
        Ok(())
    }

    #[tokio::test]
    async fn root_fields_are_documented_with_examples() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let query = r#"
          query {
            queryRoot: __type(name: "QueryRoot") {
              description
              fields { name description }
            }
            mutationRoot: __type(name: "MutationRoot") {
              description
              fields { name description }
            }
          }
        "#;

        let response = schema.execute(query).await;
        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let data = response.data.into_json()?;

        for root_key in ["queryRoot", "mutationRoot"] {
            let root_desc = data[root_key]["description"].as_str().unwrap_or_default();
            assert!(
                root_desc.contains("Usage notes:"),
                "{root_key} missing usage notes"
            );
            assert!(root_desc.contains("Example:"), "{root_key} missing example");

            let fields = data[root_key]["fields"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing fields for {root_key}"))?;

            for field in fields {
                let name = field["name"].as_str().unwrap_or("<unknown>");
                let description = field["description"].as_str().unwrap_or_default();
                assert!(
                    !description.is_empty(),
                    "{root_key}.{name} is missing a description"
                );
                assert!(
                    description.contains("Example:"),
                    "{root_key}.{name} is missing an example"
                );
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn core_object_types_have_usage_docs() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let query = r#"
          query {
            __schema {
              types {
                name
                description
              }
            }
          }
        "#;

        let response = schema.execute(query).await;
        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let data = response.data.into_json()?;

        let docs = data["__schema"]["types"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing __schema.types"))?
            .iter()
            .filter_map(|entry| {
                Some((
                    entry.get("name")?.as_str()?.to_string(),
                    entry
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ))
            })
            .collect::<std::collections::HashMap<_, _>>();

        let required = [
            "Actor",
            "Behavior",
            "Session",
            "SessionMessage",
            "Port",
            "PortBinding",
            "PortActorBinding",
            "Provider",
            "App",
            "AppCapability",
            "AppConnection",
            "AppSecret",
            "ClockworkJob",
            "ClockworkJobRun",
            "Task",
            "TaskComment",
            "TaskEvent",
            "MemoryEntity",
            "MemoryFact",
            "Policy",
            "PolicyUse",
            "User",
        ];

        for name in required {
            let description = docs
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("missing type {name}"))?;
            assert!(!description.is_empty(), "{name} missing description");
            assert!(
                description.contains("Example:"),
                "{name} description missing example"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn subscription_session_chat_streams_new_messages() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let data = schema.data::<BorgGqlData>().expect("gql data").clone();

        let session_id = Uri::from_parts("borg", "session", Some("sub-chat"))?;
        let user_id = Uri::from_parts("borg", "user", Some("sub-user"))?;
        let port_id = Uri::from_parts("borg", "port", Some("http"))?;
        data.db
            .upsert_session(&session_id, &[user_id], &port_id)
            .await?;

        let request = async_graphql::Request::new(
            r#"
              subscription($session: Uri!) {
                sessionChat(sessionId: $session, afterMessageIndex: -1, pollIntervalMs: 100) {
                  messageIndex
                  messageType
                  role
                  text
                }
              }
            "#,
        )
        .variables(async_graphql::Variables::from_json(
            json!({ "session": session_id }),
        ));

        let mut stream = schema.execute_stream(request);

        data.db
            .append_session_message(
                &session_id,
                &json!({"type":"assistant","role":"assistant","content":"hello from subscription"}),
            )
            .await?;

        let response = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for subscription event"))?
            .ok_or_else(|| anyhow::anyhow!("subscription ended unexpectedly"))?;

        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let payload = response.data.into_json()?;
        assert_eq!(payload["sessionChat"]["messageType"], "assistant");
        assert_eq!(payload["sessionChat"]["role"], "assistant");
        assert_eq!(payload["sessionChat"]["text"], "hello from subscription");
        Ok(())
    }

    #[tokio::test]
    async fn subscription_notifications_filter_user_messages_by_default() -> anyhow::Result<()> {
        let schema = test_schema().await?;
        let data = schema.data::<BorgGqlData>().expect("gql data").clone();

        let session_id = Uri::from_parts("borg", "session", Some("sub-notifications"))?;
        let user_id = Uri::from_parts("borg", "user", Some("sub-user-2"))?;
        let port_id = Uri::from_parts("borg", "port", Some("http"))?;
        data.db
            .upsert_session(&session_id, &[user_id], &port_id)
            .await?;

        let request = async_graphql::Request::new(
            r#"
              subscription($session: Uri!) {
                sessionNotifications(sessionId: $session, afterMessageIndex: -1, pollIntervalMs: 100) {
                  kind
                  messageType
                  role
                  text
                }
              }
            "#,
        )
        .variables(async_graphql::Variables::from_json(
            json!({ "session": session_id }),
        ));

        let mut stream = schema.execute_stream(request);

        data.db
            .append_session_message(
                &session_id,
                &json!({"type":"user","role":"user","content":"user message"}),
            )
            .await?;
        data.db
            .append_session_message(
                &session_id,
                &json!({"type":"assistant","role":"assistant","content":"assistant notification"}),
            )
            .await?;

        let response = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for notification event"))?
            .ok_or_else(|| anyhow::anyhow!("subscription ended unexpectedly"))?;

        assert!(response.errors.is_empty(), "{:#?}", response.errors);
        let payload = response.data.into_json()?;
        assert_eq!(payload["sessionNotifications"]["kind"], "ASSISTANT_REPLY");
        assert_eq!(payload["sessionNotifications"]["messageType"], "assistant");
        assert_eq!(payload["sessionNotifications"]["role"], "assistant");
        assert_eq!(
            payload["sessionNotifications"]["text"],
            "assistant notification"
        );
        Ok(())
    }

    #[test]
    fn static_schema_snapshot_is_generated() {
        let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema.graphql");
        let schema = std::fs::read_to_string(schema_path).expect("generated schema.graphql");
        assert!(schema.contains("interface Node"));
        assert!(schema.contains("type MutationRoot"));
        assert!(schema.contains("type SubscriptionRoot"));
    }
}
