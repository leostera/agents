use std::time::Duration;

use async_graphql::futures_util::stream::{self, BoxStream};
use async_graphql::futures_util::{Stream, StreamExt};
use async_graphql::{
    Context, Description, Enum, Error, ErrorExtensions, InputObject, Interface, Object,
    Result as GqlResult, SimpleObject, Subscription,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use borg_core::{Entity, EntityPropValue, Uri};
use borg_db::{
    ActorMessageRecord, ActorRecord, AppCapabilityRecord, AppConnectionRecord, AppRecord,
    AppSecretRecord, CreateScheduleJobInput, PortRecord, ProviderRecord, ScheduleJobRecord,
    ScheduleJobRunRecord, UpdateScheduleJobInput,
};
use borg_llm::Provider as LlmProvider;
use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::providers::openrouter::OpenRouterProvider;
use borg_memory::{FactArity, FactRecord, FactValue, SearchQuery, Uri as MemoryUri};
use borg_taskgraph::{
    CreateTaskInput, EventRecord, ListParams, TaskGraphStore, TaskPatch, TaskRecord, TaskStatus,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::context::{
    BorgGqlData, DEFAULT_SUBSCRIPTION_POLL_MS, MAX_SUBSCRIPTION_POLL_MS, MIN_SUBSCRIPTION_POLL_MS,
};
use crate::scalars::{JsonValue, UriScalar};

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
        /// Relay-style edge carrying one node plus cursor for forward pagination.
        struct $edge {
            /// Opaque edge cursor to pass back into `after`.
            cursor: String,
            /// Materialized node for this edge.
            node: $node_ty,
        }

        #[derive(SimpleObject, Clone)]
        /// Relay-style page container for cursor-based list traversal.
        struct $conn {
            /// Returned edges for the current page.
            edges: Vec<$edge>,
            /// Pagination state for the current page.
            page_info: PageInfo,
        }
    };
}

connection_types!(ActorEdge, ActorConnection, ActorObject);
connection_types!(ActorMessageEdge, ActorMessageConnection, ActorMessageObject);
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
connection_types!(ScheduleJobEdge, ScheduleJobConnection, ScheduleJobObject);
connection_types!(
    ScheduleJobRunEdge,
    ScheduleJobRunConnection,
    ScheduleJobRunObject
);
connection_types!(TaskEdge, TaskConnection, TaskObject);
connection_types!(TaskCommentEdge, TaskCommentConnection, TaskCommentObject);
connection_types!(TaskEventEdge, TaskEventConnection, TaskEventObject);
connection_types!(MemoryEntityEdge, MemoryEntityConnection, MemoryEntityObject);
connection_types!(MemoryFactEdge, MemoryFactConnection, MemoryFactObject);

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

async fn fetch_provider_models(record: &ProviderRecord) -> GqlResult<Vec<ModelObject>> {
    let provider_kind = record.provider_kind.trim().to_ascii_lowercase();

    let mut model_names = match provider_kind.as_str() {
        "openai" => {
            let mut builder = OpenAiProvider::build().api_key(record.api_key.clone());
            if let Some(base_url) = record.base_url.as_ref() {
                builder = builder.base_url(base_url.clone());
            }
            if let Some(default_text_model) = record.default_text_model.as_ref() {
                builder = builder.chat_completions_model(default_text_model.clone());
            }
            if let Some(default_audio_model) = record.default_audio_model.as_ref() {
                builder = builder.audio_transcriptions_model(default_audio_model.clone());
            }
            let provider = builder
                .build()
                .map_err(|err| gql_error_with_code(err.to_string(), "BAD_REQUEST"))?;
            provider
                .available_models()
                .await
                .map_err(|err| gql_error_with_code(err.to_string(), "INTERNAL"))?
        }
        "openrouter" => {
            let mut builder = OpenRouterProvider::build().api_key(record.api_key.clone());
            if let Some(base_url) = record.base_url.as_ref() {
                builder = builder.base_url(base_url.clone());
            }
            if let Some(default_text_model) = record.default_text_model.as_ref() {
                builder = builder.chat_completions_model(default_text_model.clone());
            }
            if let Some(default_audio_model) = record.default_audio_model.as_ref() {
                builder = builder.audio_transcriptions_model(default_audio_model.clone());
            }
            let provider = builder
                .build()
                .map_err(|err| gql_error_with_code(err.to_string(), "BAD_REQUEST"))?;
            provider
                .available_models()
                .await
                .map_err(|err| gql_error_with_code(err.to_string(), "INTERNAL"))?
        }
        _ => Vec::new(),
    };

    if let Some(default_model) = record.default_text_model.as_ref()
        && !default_model.trim().is_empty()
    {
        model_names.push(default_model.clone());
    }

    model_names.sort();
    model_names.dedup();

    Ok(model_names
        .into_iter()
        .map(|name| ModelObject { name })
        .collect())
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

async fn resolve_actor_stream_start_offset(
    data: &BorgGqlData,
    actor_id: &Uri,
    after_message_id: Option<&Uri>,
) -> GqlResult<usize> {
    let actor_exists = data
        .db
        .get_actor(actor_id)
        .await
        .map_err(map_anyhow)?
        .is_some();
    if !actor_exists {
        return Err(gql_error_with_code("actor not found", "NOT_FOUND"));
    }

    if let Some(after_id) = after_message_id {
        let offset = data
            .db
            .get_actor_message_offset_by_id(actor_id, after_id)
            .await
            .map_err(map_anyhow)?;
        if let Some(offset) = offset {
            return Ok(offset.saturating_add(1));
        }
        return Err(gql_error_with_code(
            "afterMessageId not found in actor history",
            "BAD_REQUEST",
        ));
    }

    Ok(0)
}

fn actor_message_subscription_stream(
    data: BorgGqlData,
    actor_id: Uri,
    start_offset: usize,
    poll_interval_ms: u64,
) -> impl Stream<Item = GqlResult<ActorMessageObject>> {
    let ticker = tokio::time::interval(Duration::from_millis(poll_interval_ms));

    stream::unfold(
        (data, actor_id, start_offset, ticker),
        |(data, actor_id, mut next_offset, mut ticker)| async move {
            loop {
                ticker.tick().await;

                let mut records = match data
                    .db
                    .list_actor_message_records(&actor_id, next_offset, 1)
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return Some((Err(map_anyhow(err)), (data, actor_id, next_offset, ticker)));
                    }
                };

                if records.is_empty() {
                    continue;
                }

                let record = records.remove(0);
                next_offset = next_offset.saturating_add(1);
                return Some((
                    Ok(ActorMessageObject::new(record)),
                    (data, actor_id, next_offset, ticker),
                ));
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

#[derive(Clone, Description)]
/// Root subscription entrypoint for real-time Borg streams.
///
/// Usage notes:
/// - Subscription transport is expected to run over WebSockets (`graphql-transport-ws`).
/// - Use `actorChat` for actor timeline streaming.
/// - Use `actorNotifications` for notification-friendly filtered events.
///
/// Example:
/// ```graphql
/// subscription($actor: Uri!) {
///   actorChat(actorId: $actor) {
///     id
///     messageType
///     role
///     text
///   }
/// }
/// ```
pub struct SubscriptionRoot;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// High-level classification for UI routing of actor notifications.
enum ActorNotificationKind {
    /// Assistant-authored response message.
    AssistantReply,
    /// Tool call or tool result activity.
    ToolActivity,
    /// Actor lifecycle or system event message.
    ActorEvent,
    /// Fallback generic message classification.
    Message,
}

#[derive(SimpleObject, Clone, Description)]
/// Notification payload projected from an actor message.
///
/// Usage notes:
/// - Notifications are fully typed and include the underlying `actorMessage`.
/// - `kind` is derived from `messageType`/`role`.
struct ActorNotificationObject {
    /// Stable notification identifier (`actorId:messageId`).
    id: String,
    /// Kind classification for UI routing and badges.
    kind: ActorNotificationKind,
    /// Short user-facing title.
    title: String,
    /// Actor URI this notification belongs to.
    actor_id: UriScalar,
    /// Source message URI.
    message_id: UriScalar,
    /// Source message type.
    message_type: String,
    /// Source role, when available.
    role: Option<String>,
    /// Source text content, when available.
    text: Option<String>,
    /// Source message timestamp.
    created_at: DateTime<Utc>,
    /// Full underlying message object.
    actor_message: ActorMessageObject,
}

impl ActorNotificationObject {
    fn from_message(message: ActorMessageObject) -> Self {
        let parsed = message.parsed();
        let (kind, title) = match parsed.message_type.as_str() {
            "assistant" => (
                ActorNotificationKind::AssistantReply,
                "Assistant reply".to_string(),
            ),
            "tool_call" => (
                ActorNotificationKind::ToolActivity,
                "Tool call requested".to_string(),
            ),
            "tool_result" => (
                ActorNotificationKind::ToolActivity,
                "Tool result received".to_string(),
            ),
            "actor_event" => (ActorNotificationKind::ActorEvent, "Actor event".to_string()),
            _ => (ActorNotificationKind::Message, "Actor message".to_string()),
        };

        Self {
            id: format!("{}:{}", message.record.actor_id, message.record.message_id),
            kind,
            title,
            actor_id: message.record.actor_id.clone().into(),
            message_id: message.record.message_id.clone().into(),
            message_type: parsed.message_type,
            role: parsed.role,
            text: parsed.text,
            created_at: message.record.created_at,
            actor_message: message,
        }
    }
}

/// Creates or updates an actor definition in Borg's control-plane graph.
///
/// Example:
/// `{ id: "borg:actor:planner", name: "Planner", status: RUNNING }`
#[derive(InputObject)]
struct UpsertActorInput {
    /// Stable actor URI (`borg:actor:*`).
    id: UriScalar,
    /// Human-readable actor name.
    name: String,
    /// Optional runtime model id (for example `gpt-4.1`).
    model: Option<String>,
    /// System prompt used when running this actor.
    system_prompt: String,
    /// Actor lifecycle status (for example `RUNNING`).
    status: ActorStatusValue,
}

/// Creates or updates a runtime ingress/egress port configuration.
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

/// Creates or updates the conversation-to-actor routing row for a port.
///
/// Example:
/// `{ portName: "telegram", conversationKey: "borg:conversation:123", actorId: "borg:actor:planner" }`
#[derive(InputObject)]
struct UpsertPortBindingInput {
    /// Port name (`http`, `telegram`, ...).
    port_name: String,
    /// Stable conversation key for ingress routing.
    conversation_key: UriScalar,
    /// Target actor URI.
    actor_id: UriScalar,
}

/// Creates or updates the conversation-to-actor override row for a port.
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

/// Creates or updates an LLM provider configuration entry.
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

/// Creates or updates an app integration definition.
///
/// Example:
/// `{ id: "borg:app:github", name: "GitHub", slug: "github", status: ACTIVE, authStrategy: "oauth2" }`
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
    status: AppStatusValue,
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

/// Creates or updates a capability exposed by an app integration.
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
    status: AppCapabilityStatusValue,
}

/// Creates or updates an external-account connection row for an app.
///
/// Example:
/// `{ appId: "borg:app:github", connectionId: "borg:app-connection:octocat", status: CONNECTED }`
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
    status: AppConnectionStatusValue,
    /// Transitional JSON metadata for this connection.
    connection: Option<JsonValue>,
}

/// Creates or updates secret material attached to an app or app connection.
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

/// Creates a new schedule scheduler job definition.
///
/// Example:
/// `{ jobId: "daily-digest", kind: "cron", actorId: "borg:actor:planner" }`
#[derive(InputObject)]
struct CreateScheduleJobInputGql {
    /// Stable job identifier.
    job_id: String,
    /// Scheduler kind (`cron`, ...).
    kind: String,
    /// Actor URI executed by this job.
    actor_id: UriScalar,
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

/// Patches mutable fields on an existing schedule scheduler job.
///
/// Usage notes:
/// - Only pass fields that need to change.
/// - `nextRunAt` set to `null` clears the existing schedule override.
#[derive(InputObject)]
struct UpdateScheduleJobInputGql {
    /// New scheduler kind.
    kind: Option<String>,
    /// New target actor URI.
    actor_id: Option<UriScalar>,
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

/// Creates a new durable taskgraph task.
///
/// Example:
/// `{ actorId: "borg:actor:creator", creatorActorId: "borg:actor:creator", assigneeActorId: "borg:actor:assignee", title: "Ship docs" }`
#[derive(InputObject)]
struct CreateTaskInputGql {
    /// Actor URI authoring the task create event.
    actor_id: UriScalar,
    /// Creator actor URI.
    creator_actor_id: UriScalar,
    /// Short task title.
    title: String,
    /// Task description/body.
    #[graphql(default)]
    description: String,
    /// Completion criteria.
    #[graphql(default)]
    definition_of_done: String,
    /// Assignee actor URI.
    assignee_actor_id: UriScalar,
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

/// Patches editable text fields on an existing taskgraph task.
///
/// Usage notes:
/// - This mutation only patches text fields.
/// - Leave fields as `null` to keep previous values.
#[derive(InputObject)]
struct UpdateTaskInputGql {
    /// Task URI to patch.
    task_id: UriScalar,
    /// Actor URI authoring the update event.
    actor_id: UriScalar,
    /// Optional new title.
    title: Option<String>,
    /// Optional new description.
    description: Option<String>,
    /// Optional new definition of done.
    definition_of_done: Option<String>,
}

/// Requests a task status transition under taskgraph rules.
///
/// Example:
/// `{ taskId: "borg:task:t1", actorId: "borg:actor:assignee", status: DOING }`
#[derive(InputObject)]
struct SetTaskStatusInput {
    /// Task URI to transition.
    task_id: UriScalar,
    /// Actor URI authoring the status transition.
    actor_id: UriScalar,
    /// Target status value.
    status: TaskStatusValue,
}

/// Future runtime input shape for direct actor chat execution.
///
/// Usage notes:
/// - Reserved for future runtime integration.
#[derive(InputObject)]
struct RunActorChatInput {
    /// Actor URI to execute.
    actor_id: UriScalar,
    /// User URI authoring the request.
    user_id: UriScalar,
    /// User text to send.
    text: String,
}

/// Future runtime input shape mirroring HTTP port execution.
///
/// Usage notes:
/// - Reserved for future runtime integration.
#[derive(InputObject)]
struct RunPortHttpInput {
    /// User URI authoring the request.
    user_id: UriScalar,
    /// User text to send.
    text: String,
    /// Optional explicit actor URI override.
    actor_id: Option<UriScalar>,
}

#[derive(Clone, Description)]
/// Runtime actor definition.
///
/// An actor is a named, long-lived Borg worker/persona with its own message
/// history.
///
/// Usage notes:
/// - Represents a runnable actor spec (`borg:actor:*`).
/// - Use `messages` for runtime timeline views.
///
/// Example:
/// ```graphql
/// { actor(id: "borg:actor:planner") { id name status } }
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

    async fn model(&self) -> Option<&str> {
        self.record.model.as_deref()
    }

    async fn default_provider_id(&self) -> Option<&str> {
        self.record.default_provider_id.as_deref()
    }

    async fn status(&self) -> ActorStatusValue {
        ActorStatusValue::from_raw(&self.record.status)
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    /// Actor history messages.
    ///
    /// Usage notes:
    /// - Backed by actor mailbox activity.
    ///
    /// Example:
    /// ```graphql
    /// { actor(id: "borg:actor:planner") { messages(first: 5) { edges { node { id messageType text } } } } }
    /// ```
    async fn messages(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ActorMessageConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let records = data
            .db
            .list_actor_message_records(&self.record.actor_id, start, first + 1)
            .await
            .map_err(map_anyhow)?;
        let has_next_page = records.len() > first;
        let edges = records
            .into_iter()
            .take(first)
            .enumerate()
            .map(|(offset, record)| ActorMessageEdge {
                cursor: encode_offset_cursor(start + offset),
                node: ActorMessageObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ActorMessageConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// Persisted timeline entry inside an actor history.
///
/// Actor messages represent user inputs, assistant outputs, tool activity, and
/// lifecycle events as ordered records.
///
/// Usage notes:
/// - Prefer `messageType`, `role`, and `text` over deprecated `payload`.
/// - `id` is the stable timeline message URI.
///
/// Example:
/// ```graphql
/// { actor(id: "borg:actor:planner") { messages(first: 5) { edges { node { id messageType role text } } } } }
/// ```
struct ActorMessageObject {
    record: ActorMessageRecord,
}

impl ActorMessageObject {
    fn new(record: ActorMessageRecord) -> Self {
        Self { record }
    }

    fn parsed(&self) -> ParsedActorMessage {
        parse_actor_message(&self.record.payload)
    }
}

#[Object(name = "ActorMessage", use_type_description)]
impl ActorMessageObject {
    async fn id(&self) -> UriScalar {
        self.record.message_id.clone().into()
    }

    async fn actor_id(&self) -> UriScalar {
        self.record.actor_id.clone().into()
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
/// External transport adapter configuration.
///
/// Ports model how Borg receives/sends traffic (for example HTTP, Telegram) and
/// how incoming conversations map into actors.
///
/// Usage notes:
/// - Ports bind external channels (`http`, `telegram`, ...) to actor routing.
/// - `bindings` and `actorBindings` expose live routing maps.
///
/// Example:
/// ```graphql
/// { port(name: "telegram") { id provider enabled bindings(first: 5) { edges { node { conversationKey actorId } } } } }
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

    async fn active_bindings(&self) -> u64 {
        self.record.active_bindings
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

    /// Actor binding rows for this port.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { bindings(first: 10) { edges { node { conversationKey actorId } } } } }
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
            .map(|(index, (conversation_key, actor_id))| PortBindingEdge {
                cursor: encode_offset_cursor(index),
                node: PortBindingObject {
                    port_name: self.record.port_name.clone(),
                    conversation_key,
                    actor_id,
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
/// Conversation routing edge from a port to an actor.
///
/// Usage notes:
/// - Canonical ingress-actor routing row.
/// - `actor` resolves the bound actor object.
///
/// Example:
/// ```graphql
/// { port(name: "telegram") { bindings(first: 5) { edges { node { conversationKey actorId actor { id } } } } } }
/// ```
struct PortBindingObject {
    port_name: String,
    conversation_key: Uri,
    actor_id: Uri,
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

    async fn actor_id(&self) -> UriScalar {
        self.actor_id.clone().into()
    }

    /// Actor bound to this conversation key, if any.
    ///
    /// Example:
    /// ```graphql
    /// { port(name: "telegram") { bindings(first: 1) { edges { node { actor { id name } } } } } }
    /// ```
    async fn actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&self.actor_id)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }
}

#[derive(Clone, Description)]
/// Conversation-specific actor override edge.
///
/// This row pins a conversation to a specific actor independently from the
/// default port actor.
///
/// Usage notes:
/// - Stores explicit actor overrides for a conversation key.
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

#[derive(SimpleObject, Clone)]
struct ModelObject {
    name: String,
}

#[derive(Clone, Description)]
/// LLM provider configuration and usage counters.
///
/// Provider rows hold credentials, default models, and operational metadata for
/// model routing.
///
/// Usage notes:
/// - `provider` is the configuration key.
/// - `providerKind` maps to adapter family when needed.
///
/// Example:
/// ```graphql
/// { provider(provider: "openai") { provider providerKind enabled defaultTextModel models { name } defaultModel { name } } }
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

    async fn default_model(&self) -> ModelObject {
        let name = self
            .record
            .default_text_model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        ModelObject { name }
    }

    async fn models(&self) -> GqlResult<Vec<ModelObject>> {
        fetch_provider_models(&self.record).await
    }
}

#[derive(Clone, Description)]
/// External app integration definition.
///
/// Apps represent integrated systems (for example GitHub) and own capabilities,
/// account connections, and secret scopes.
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

    async fn status(&self) -> AppStatusValue {
        AppStatusValue::from_raw(&self.record.status)
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
/// App operation that can be invoked by runtime/tooling.
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

    async fn status(&self) -> AppCapabilityStatusValue {
        AppCapabilityStatusValue::from_raw(&self.record.status)
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }
}

#[derive(Clone, Description)]
/// Linked external account for an app integration.
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

    async fn status(&self) -> AppConnectionStatusValue {
        AppConnectionStatusValue::from_raw(&self.record.status)
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
/// Secret row scoped to an app or a specific app connection.
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
/// Durable scheduler job definition for automated actor execution.
///
/// Usage notes:
/// - Defines recurring/queued actor execution plans.
/// - Use `runs` to inspect execution history.
///
/// Example:
/// ```graphql
/// { scheduleJob(jobId: "daily-digest") { id kind status nextRunAt runs(first: 5) { edges { node { id firedAt } } } } }
/// ```
struct ScheduleJobObject {
    record: ScheduleJobRecord,
}

impl ScheduleJobObject {
    fn new(record: ScheduleJobRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ScheduleJob", use_type_description)]
impl ScheduleJobObject {
    async fn id(&self) -> &str {
        &self.record.job_id
    }

    async fn kind(&self) -> &str {
        &self.record.kind
    }

    async fn status(&self) -> ScheduleJobStatusValue {
        ScheduleJobStatusValue::from_raw(&self.record.status)
    }

    async fn target_actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.target_actor_id).ok().map(UriScalar)
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

    /// Historical run rows for this schedule job.
    ///
    /// Example:
    /// ```graphql
    /// { scheduleJob(jobId: "daily") { runs(first: 10) { edges { node { id firedAt messageId } } } } }
    /// ```
    async fn runs(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ScheduleJobRunConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let runs = data
            .db
            .list_schedule_job_runs(&self.record.job_id, fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(runs, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ScheduleJobRunEdge {
                cursor: encode_offset_cursor(index),
                node: ScheduleJobRunObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ScheduleJobRunConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}

#[derive(Clone, Description)]
/// Immutable execution record emitted when a schedule job fires.
///
/// Usage notes:
/// - Immutable execution row emitted by schedule runtime.
///
/// Example:
/// ```graphql
/// { scheduleJob(jobId: "daily-digest") { runs(first: 5) { edges { node { id scheduledFor firedAt messageId } } } } }
/// ```
struct ScheduleJobRunObject {
    record: ScheduleJobRunRecord,
}

impl ScheduleJobRunObject {
    fn new(record: ScheduleJobRunRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ScheduleJobRun", use_type_description)]
impl ScheduleJobRunObject {
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

    async fn message_id(&self) -> &str {
        &self.record.message_id
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }
}

#[derive(Clone, Description)]
/// Durable taskgraph work item.
///
/// Tasks carry assignment, dependencies, audit history, and review lifecycle
/// state for explicit multi-step work.
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

    async fn assignee_actor_id(&self) -> &str {
        &self.record.assignee_actor_id
    }

    async fn reviewer_actor_id(&self) -> &str {
        &self.record.reviewer_actor_id
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
    /// { task(id: "borg:task:t1") { comments(first: 20) { edges { node { id body authorActorId } } } } }
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
/// Human/agent comment attached to a task timeline.
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

    /// Actor URI that authored this comment.
    async fn author_actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.author_actor_id).ok().map(UriScalar)
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
/// Structured audit event emitted by taskgraph transitions.
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

    /// Actor URI that triggered the event.
    async fn actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.actor_id).ok().map(UriScalar)
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
/// Typed projection of task event payload details.
///
/// Usage notes:
/// - `kind` indicates which subset of optional fields is populated.
/// - Consumers should branch on `kind` before reading event-specific fields.
struct TaskEventDataObject {
    /// Event type/kind copied from the event row.
    kind: String,
    /// New assignee actor ID when relevant.
    assignee_actor_id: Option<String>,
    /// Reviewer actor ID when relevant.
    reviewer_actor_id: Option<String>,
    /// Parent task URI when relevant.
    parent_uri: Option<String>,
    /// Updated title when relevant.
    title: Option<String>,
    /// Updated description when relevant.
    description: Option<String>,
    /// Updated definition-of-done when relevant.
    definition_of_done: Option<String>,
    /// Previous assignee actor ID when relevant.
    old_assignee_actor_id: Option<String>,
    /// Replacement assignee actor ID when relevant.
    new_assignee_actor_id: Option<String>,
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
    assignee_actor_id: Option<String>,
    #[serde(default)]
    reviewer_actor_id: Option<String>,
    #[serde(default)]
    parent_uri: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    definition_of_done: Option<String>,
    #[serde(default)]
    old_assignee_actor_id: Option<String>,
    #[serde(default)]
    new_assignee_actor_id: Option<String>,
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
            assignee_actor_id: parsed.assignee_actor_id,
            reviewer_actor_id: parsed.reviewer_actor_id,
            parent_uri: parsed.parent_uri,
            title: parsed.title,
            description: parsed.description,
            definition_of_done: parsed.definition_of_done,
            old_assignee_actor_id: parsed.old_assignee_actor_id,
            new_assignee_actor_id: parsed.new_assignee_actor_id,
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
/// Canonical entity node in Borg long-term memory.
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
/// One typed property entry on a memory entity.
struct MemoryPropertyObject {
    /// Property key name.
    key: String,
    /// Typed property value.
    value: MemoryValueObject,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Type discriminator for normalized memory values.
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
/// Normalized typed value used by memory properties and facts.
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
/// Cardinality contract for a memory fact field.
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
/// Immutable fact assertion row in long-term memory storage.
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

#[derive(Interface, Clone)]
#[graphql(field(name = "id", ty = "UriScalar"))]
/// Unified node interface for cross-entity graph traversal.
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
enum Node {
    Actor(ActorObject),
    Port(PortObject),
    Provider(ProviderObject),
    App(AppObject),
    Task(TaskObject),
    MemoryEntity(MemoryEntityObject),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Lifecycle states for actor records.
enum ActorStatusValue {
    /// Actor can receive and execute new work.
    Running,
    /// Actor is intentionally paused.
    Paused,
    /// Actor is disabled and should not run.
    Disabled,
    /// Actor encountered a terminal/error state.
    Error,
    /// Actor row contains an unrecognized status value.
    Unknown,
}

impl ActorStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "disabled" => Self::Disabled,
            "error" | "failed" => Self::Error,
            _ => Self::Unknown,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Paused => "PAUSED",
            Self::Disabled => "DISABLED",
            Self::Error => "ERROR",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Lifecycle states for app integration definitions.
enum AppStatusValue {
    /// App integration is enabled and available.
    Active,
    /// App integration exists but is currently inactive.
    Inactive,
    /// App integration is disabled.
    Disabled,
    /// App integration is archived and preserved for history.
    Archived,
    /// App row contains an unrecognized status value.
    Unknown,
}

impl AppStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "active" => Self::Active,
            "inactive" => Self::Inactive,
            "disabled" => Self::Disabled,
            "archived" => Self::Archived,
            _ => Self::Unknown,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Disabled => "disabled",
            Self::Archived => "archived",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Lifecycle states for app capability rows.
enum AppCapabilityStatusValue {
    /// Capability is enabled and invokable.
    Active,
    /// Capability exists but is currently inactive.
    Inactive,
    /// Capability is disabled.
    Disabled,
    /// Capability is deprecated and retained for compatibility.
    Deprecated,
    /// Capability row contains an unrecognized status value.
    Unknown,
}

impl AppCapabilityStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "active" => Self::Active,
            "inactive" => Self::Inactive,
            "disabled" => Self::Disabled,
            "deprecated" => Self::Deprecated,
            _ => Self::Unknown,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Disabled => "disabled",
            Self::Deprecated => "deprecated",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Connection states for external app accounts.
enum AppConnectionStatusValue {
    /// Connection is healthy and ready for use.
    Connected,
    /// Connection exists but is not currently authenticated.
    Disconnected,
    /// Connection setup is in progress.
    Pending,
    /// Connection exists but is intentionally revoked.
    Revoked,
    /// Connection is in an error state.
    Error,
    /// Connection row contains an unrecognized status value.
    Unknown,
}

impl AppConnectionStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "connected" => Self::Connected,
            "disconnected" => Self::Disconnected,
            "pending" => Self::Pending,
            "revoked" => Self::Revoked,
            "error" | "failed" => Self::Error,
            _ => Self::Unknown,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::Pending => "pending",
            Self::Revoked => "revoked",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Runtime lifecycle states for scheduler jobs.
enum ScheduleJobStatusValue {
    /// Job is active and eligible to run.
    Active,
    /// Job is paused and should not be scheduled.
    Paused,
    /// Job has been cancelled.
    Cancelled,
    /// Job completed and will not run again.
    Completed,
    /// Job row contains an unrecognized status value.
    Unknown,
}

impl ScheduleJobStatusValue {
    fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "cancelled" => Self::Cancelled,
            "completed" => Self::Completed,
            _ => Self::Unknown,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
            Self::Unknown => "unknown",
        }
    }
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
/// Future runtime response contract for direct actor chat execution.
struct RunActorChatResult {
    /// Whether the runtime call succeeded.
    ok: bool,
    /// Human-readable status message.
    message: String,
}

#[derive(SimpleObject)]
/// Future runtime response contract for HTTP-port style execution.
struct RunPortHttpResult {
    /// Whether the runtime call succeeded.
    ok: bool,
    /// Human-readable status message.
    message: String,
}

#[derive(Default)]
struct ParsedActorMessage {
    message_type: String,
    role: Option<String>,
    text: Option<String>,
}

fn parse_actor_message(payload: &serde_json::Value) -> ParsedActorMessage {
    let Some(obj) = payload.as_object() else {
        return ParsedActorMessage {
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
            "actor_event" => Some("event".to_string()),
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

    ParsedActorMessage {
        message_type,
        role,
        text,
    }
}

mod resolvers;
