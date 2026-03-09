use std::time::Duration;

use async_graphql::futures_util::stream::{self, BoxStream};
use async_graphql::futures_util::{Stream, StreamExt};
use async_graphql::{
    Context, Enum, Error, ErrorExtensions, InputObject, Interface, Object, Result as GqlResult,
    SimpleObject, Subscription,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use borg_core::{Entity, EntityPropValue, MessagePayload, Uri};
use borg_db::{
    ActorRecord, AppCapabilityRecord, AppConnectionRecord, AppRecord, AppSecretRecord,
    CreateScheduleJobInput, MessageRecord, PortRecord, ProviderRecord, ScheduleJobRecord,
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
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

macro_rules! connection_types {
    ($edge:ident, $conn:ident, $node_ty:ty) => {
        #[derive(SimpleObject, Clone)]
        struct $edge {
            cursor: String,
            node: $node_ty,
        }

        #[derive(SimpleObject, Clone)]
        struct $conn {
            edges: Vec<$edge>,
            page_info: PageInfo,
        }
    };
}

connection_types!(ActorEdge, ActorConnection, ActorObject);
connection_types!(ActorMessageEdge, ActorMessageConnection, ActorMessageObject);
connection_types!(PortEdge, PortConnection, PortObject);
connection_types!(PortBindingEdge, PortBindingConnection, PortBindingObject);
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

#[derive(SimpleObject, Clone)]
struct AvailableCapabilityObject {
    name: String,
    description: String,
}

#[derive(SimpleObject, Clone)]
struct ContextToolSpecObject {
    name: String,
    description: String,
    parameters: JsonValue,
}

#[derive(SimpleObject, Clone)]
struct ContextMessageObject {
    #[graphql(name = "type")]
    message_type: String,
    content: Option<String>,
    role: Option<String>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    arguments: Option<JsonValue>,
    result: Option<JsonValue>,
    is_error: Option<bool>,
}

#[derive(SimpleObject, Clone)]
struct ContextWindowObject {
    system_prompt: String,
    behavior_prompt: String,
    available_tools: Vec<ContextToolSpecObject>,
    available_capabilities: Vec<AvailableCapabilityObject>,
    ordered_messages: Vec<ContextMessageObject>,
}

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
    _after_message_id: Option<&Uri>,
) -> GqlResult<usize> {
    let actor_exists = data
        .db
        .get_actor(&borg_core::ActorId(actor_id.clone()))
        .await
        .map_err(map_anyhow)?
        .is_some();
    if !actor_exists {
        return Err(gql_error_with_code("actor not found", "NOT_FOUND"));
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

                // Stream both pending and processed messages
                let mut records = match data
                    .db
                    .list_messages(&actor_id.clone().into(), next_offset + 1)
                    .await
                {
                    Ok(value) => value,
                    Err(err) => {
                        return Some((Err(map_anyhow(err)), (data, actor_id, next_offset, ticker)));
                    }
                };

                if records.len() <= next_offset {
                    continue;
                }

                let record = records.remove(next_offset);
                next_offset = next_offset.saturating_add(1);
                return Some((
                    Ok(ActorMessageObject::new(record)),
                    (data, actor_id, next_offset, ticker),
                ));
            }
        },
    )
}

#[derive(Clone)]
pub struct QueryRoot;
#[derive(Clone)]
pub struct MutationRoot;
#[derive(Clone)]
pub struct SubscriptionRoot;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum ActorNotificationKind {
    AssistantReply,
    ToolActivity,
    ActorEvent,
    Message,
}

#[derive(SimpleObject, Clone)]
struct ActorNotificationObject {
    id: String,
    kind: ActorNotificationKind,
    title: String,
    actor_id: UriScalar,
    message_id: UriScalar,
    message_type: String,
    role: Option<String>,
    text: Option<String>,
    created_at: DateTime<Utc>,
    actor_message: ActorMessageObject,
}

impl ActorNotificationObject {
    fn from_message(message: ActorMessageObject) -> Self {
        let (kind, title) = match message.record.payload.kind() {
            "final_assistant_message" | "assistant_text" => (
                ActorNotificationKind::AssistantReply,
                "Assistant reply".to_string(),
            ),
            "tool_call" | "tool_result" => (
                ActorNotificationKind::ToolActivity,
                "Tool activity".to_string(),
            ),
            _ => (ActorNotificationKind::Message, "Actor message".to_string()),
        };

        Self {
            id: format!(
                "{}:{}",
                message.record.receiver_id, message.record.message_id
            ),
            kind,
            title,
            actor_id: message.record.receiver_id.clone().into_uri().into(),
            message_id: message.record.message_id.clone().into(),
            message_type: message.record.payload.kind().to_string(),
            role: None,
            text: None,
            created_at: message.record.delivered_at,
            actor_message: message,
        }
    }
}

#[derive(InputObject)]
struct UpsertActorInput {
    id: UriScalar,
    name: String,
    model: Option<String>,
    system_prompt: String,
    status: ActorStatusValue,
}

#[derive(InputObject)]
struct UpsertPortInput {
    name: String,
    provider: String,
    enabled: bool,
    allows_guests: bool,
    assigned_actor_id: Option<UriScalar>,
    settings: Option<JsonValue>,
}

#[derive(InputObject)]
struct UpsertPortBindingInput {
    port_name: String,
    conversation_key: UriScalar,
    actor_id: UriScalar,
}

#[derive(InputObject)]
struct UpsertPortActorBindingInput {
    port_name: String,
    conversation_key: UriScalar,
    actor_id: Option<UriScalar>,
}

#[derive(InputObject)]
struct UpsertProviderInput {
    provider: String,
    provider_kind: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    enabled: Option<bool>,
    default_text_model: Option<String>,
    default_audio_model: Option<String>,
}

#[derive(InputObject)]
struct UpsertAppInput {
    id: UriScalar,
    name: String,
    slug: String,
    description: String,
    status: AppStatusValue,
    #[graphql(default)]
    built_in: bool,
    source: String,
    auth_strategy: String,
    auth_config: Option<JsonValue>,
    #[graphql(default)]
    available_secrets: Vec<String>,
}

#[derive(InputObject)]
struct UpsertAppCapabilityInput {
    app_id: UriScalar,
    capability_id: UriScalar,
    name: String,
    hint: String,
    mode: String,
    instructions: String,
    status: AppCapabilityStatusValue,
}

#[derive(InputObject)]
struct UpsertAppConnectionInput {
    app_id: UriScalar,
    connection_id: UriScalar,
    owner_user_id: Option<UriScalar>,
    provider_account_id: Option<String>,
    external_user_id: Option<String>,
    status: AppConnectionStatusValue,
    connection: Option<JsonValue>,
}

#[derive(InputObject)]
struct UpsertAppSecretInput {
    app_id: UriScalar,
    secret_id: UriScalar,
    connection_id: Option<UriScalar>,
    key: String,
    value: String,
    kind: String,
}

#[derive(InputObject)]
struct CreateScheduleJobInputGql {
    job_id: String,
    kind: String,
    actor_id: UriScalar,
    message_type: String,
    payload: JsonValue,
    headers: Option<JsonValue>,
    schedule_spec: JsonValue,
    next_run_at: Option<String>,
}

#[derive(InputObject)]
struct UpdateScheduleJobInputGql {
    kind: Option<String>,
    actor_id: Option<UriScalar>,
    message_type: Option<String>,
    payload: Option<JsonValue>,
    headers: Option<JsonValue>,
    schedule_spec: Option<JsonValue>,
    next_run_at: Option<String>,
}

#[derive(InputObject)]
struct CreateTaskInputGql {
    actor_id: UriScalar,
    creator_actor_id: UriScalar,
    title: String,
    #[graphql(default)]
    description: String,
    #[graphql(default)]
    definition_of_done: String,
    assignee_actor_id: UriScalar,
    parent_uri: Option<UriScalar>,
    #[graphql(default)]
    blocked_by: Vec<UriScalar>,
    #[graphql(default)]
    references: Vec<UriScalar>,
    #[graphql(default)]
    labels: Vec<String>,
}

#[derive(InputObject)]
struct UpdateTaskInputGql {
    task_id: UriScalar,
    actor_id: UriScalar,
    title: Option<String>,
    description: Option<String>,
    definition_of_done: Option<String>,
}

#[derive(InputObject)]
struct SetTaskStatusInput {
    task_id: UriScalar,
    actor_id: UriScalar,
    status: TaskStatusValue,
}

#[derive(InputObject)]
struct RunActorChatInput {
    actor_id: UriScalar,
    user_id: UriScalar,
    text: String,
}

#[derive(InputObject)]
struct RunPortHttpInput {
    user_id: UriScalar,
    text: String,
    actor_id: Option<UriScalar>,
}

#[derive(Clone)]
struct ActorObject {
    record: ActorRecord,
}

impl ActorObject {
    fn new(record: ActorRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Actor")]
impl ActorObject {
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

    async fn status(&self) -> ActorStatusValue {
        ActorStatusValue::from_raw(&self.record.status)
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    async fn messages(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ActorMessageConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;

        // Show all messages (pending + processed) for the actor mailbox
        let records = data
            .db
            .list_messages(&self.record.actor_id.clone().into(), first + 1)
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

    async fn context_window(&self, ctx: &Context<'_>) -> GqlResult<ContextWindowObject> {
        let data = ctx_data(ctx)?;
        let runtime = data
            .runtime
            .as_ref()
            .ok_or_else(|| map_anyhow(anyhow::anyhow!("runtime unavailable")))?;
        let context: borg_agent::ContextWindow<
            borg_agent::BorgToolCall,
            borg_agent::BorgToolResult,
        > = data
            .supervisor
            .inspect_actor_context(&self.record.actor_id, runtime.clone())
            .await
            .map_err(map_anyhow)?;

        Ok(ContextWindowObject {
            system_prompt: context.system_prompt,
            behavior_prompt: context.behavior_prompt,
            available_tools: context
                .available_tools
                .into_iter()
                .map(|t| ContextToolSpecObject {
                    name: t.name,
                    description: t.description,
                    parameters: JsonValue(t.parameters),
                })
                .collect(),
            available_capabilities: context
                .available_capabilities
                .into_iter()
                .map(|c| AvailableCapabilityObject {
                    name: c.name,
                    description: c.description,
                })
                .collect(),
            ordered_messages: context
                .ordered_messages
                .into_iter()
                .map(map_agent_message)
                .collect(),
        })
    }
}

fn map_agent_message(
    msg: borg_agent::Message<borg_agent::BorgToolCall, borg_agent::BorgToolResult>,
) -> ContextMessageObject {
    match msg {
        borg_agent::Message::System { content } => ContextMessageObject {
            message_type: "system".to_string(),
            content: Some(content),
            role: Some("system".to_string()),
            tool_call_id: None,
            tool_name: None,
            arguments: None,
            result: None,
            is_error: None,
        },
        borg_agent::Message::User { content } => ContextMessageObject {
            message_type: "user".to_string(),
            content: Some(content),
            role: Some("user".to_string()),
            tool_call_id: None,
            tool_name: None,
            arguments: None,
            result: None,
            is_error: None,
        },
        borg_agent::Message::UserAudio {
            file_id,
            transcript,
            ..
        } => ContextMessageObject {
            message_type: "user_audio".to_string(),
            content: Some(format!("Audio: {} (Transcript: {})", file_id, transcript)),
            role: Some("user".to_string()),
            tool_call_id: None,
            tool_name: None,
            arguments: None,
            result: None,
            is_error: None,
        },
        borg_agent::Message::Assistant { content } => ContextMessageObject {
            message_type: "assistant".to_string(),
            content: Some(content),
            role: Some("assistant".to_string()),
            tool_call_id: None,
            tool_name: None,
            arguments: None,
            result: None,
            is_error: None,
        },
        borg_agent::Message::ToolCall {
            tool_call_id,
            name,
            arguments,
        } => ContextMessageObject {
            message_type: "tool_call".to_string(),
            content: None,
            role: Some("assistant".to_string()),
            tool_call_id: Some(tool_call_id),
            tool_name: Some(name),
            arguments: Some(JsonValue(arguments.to_value().unwrap_or_default())),
            result: None,
            is_error: None,
        },
        borg_agent::Message::ToolResult {
            tool_call_id,
            name,
            content,
        } => {
            let (result, is_error) = match content {
                borg_agent::ToolOutputEnvelope::Ok(res) => (
                    JsonValue(serde_json::to_value(res).unwrap_or_default()),
                    false,
                ),
                borg_agent::ToolOutputEnvelope::ByDesign(res) => (
                    JsonValue(serde_json::to_value(res).unwrap_or_default()),
                    false,
                ),
                borg_agent::ToolOutputEnvelope::Error(err) => (JsonValue(json!(err)), true),
            };
            ContextMessageObject {
                message_type: "tool_result".to_string(),
                content: None,
                role: Some("tool".to_string()),
                tool_call_id: Some(tool_call_id),
                tool_name: Some(name),
                arguments: None,
                result: Some(result),
                is_error: Some(is_error),
            }
        }
        borg_agent::Message::ActorEvent { name, .. } => ContextMessageObject {
            message_type: "actor_event".to_string(),
            content: Some(format!("Event: {}", name)),
            role: Some("system".to_string()),
            tool_call_id: None,
            tool_name: None,
            arguments: None,
            result: None,
            is_error: None,
        },
    }
}

#[derive(Clone)]
struct ActorMessageObject {
    record: MessageRecord,
}

impl ActorMessageObject {
    fn new(record: MessageRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ActorMessage")]
impl ActorMessageObject {
    async fn id(&self) -> UriScalar {
        self.record.message_id.clone().into()
    }

    async fn actor_id(&self) -> UriScalar {
        self.record.receiver_id.clone().into()
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.record.delivered_at
    }

    async fn message_type(&self) -> String {
        self.record.payload.kind().to_string()
    }

    async fn role(&self) -> Option<String> {
        match &self.record.payload {
            MessagePayload::UserText(_) => Some("user".to_string()),
            MessagePayload::AssistantText(_) => Some("assistant".to_string()),
            MessagePayload::FinalAssistantMessage(_) => Some("assistant".to_string()),
            MessagePayload::ToolCall(_) => Some("tool".to_string()),
            MessagePayload::ToolResult(_) => Some("tool".to_string()),
            _ => None,
        }
    }

    async fn text(&self) -> Option<String> {
        match &self.record.payload {
            MessagePayload::UserText(p) => Some(p.text.clone()),
            MessagePayload::AssistantText(p) => Some(p.text.clone()),
            MessagePayload::FinalAssistantMessage(p) => Some(p.text.clone()),
            _ => None,
        }
    }

    async fn payload(&self) -> JsonValue {
        JsonValue(serde_json::to_value(&self.record.payload).unwrap_or(json!({})))
    }
}

#[derive(Clone)]
struct PortObject {
    record: PortRecord,
}

impl PortObject {
    fn new(record: PortRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "Port")]
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
        self.record.assigned_actor_id.clone().map(|id| id.into())
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.record.updated_at
    }

    async fn settings(&self) -> JsonValue {
        JsonValue(self.record.settings.clone())
    }

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

    async fn bindings(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PortBindingConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;

        let bindings = data
            .db
            .list_port_bindings(&self.record.port_id, first + 1)
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(bindings, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| PortBindingEdge {
                cursor: encode_offset_cursor(index),
                node: PortBindingObject {
                    port_name: self.record.port_name.clone(),
                    conversation_key: record.conversation_key,
                    actor_id: record.actor_id.into_uri(),
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
}

#[derive(Clone)]
struct PortBindingObject {
    port_name: String,
    conversation_key: String,
    actor_id: Uri,
}

#[Object(name = "PortBinding")]
impl PortBindingObject {
    async fn id(&self) -> String {
        format!("{}:{}", self.port_name, self.conversation_key)
    }

    async fn port_name(&self) -> &str {
        &self.port_name
    }

    async fn conversation_key(&self) -> &str {
        &self.conversation_key
    }

    async fn actor_id(&self) -> UriScalar {
        self.actor_id.clone().into()
    }

    async fn actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&self.actor_id.clone().into())
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }
}

#[derive(Clone)]
struct PortActorBindingObject {
    port_name: String,
    conversation_key: String,
    actor_id: Option<Uri>,
}

#[Object(name = "PortActorBinding")]
impl PortActorBindingObject {
    async fn id(&self) -> String {
        format!("{}:{}", self.port_name, self.conversation_key)
    }

    async fn port_name(&self) -> &str {
        &self.port_name
    }

    async fn conversation_key(&self) -> &str {
        &self.conversation_key
    }

    async fn actor_id(&self) -> Option<UriScalar> {
        self.actor_id.clone().map(UriScalar)
    }

    async fn actor(&self, ctx: &Context<'_>) -> GqlResult<Option<ActorObject>> {
        let Some(actor_id) = self.actor_id.as_ref() else {
            return Ok(None);
        };
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&actor_id.clone().into())
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }
}

#[derive(SimpleObject, Clone)]
struct ModelObject {
    name: String,
}

#[derive(Clone)]
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

#[Object(name = "Provider")]
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

#[derive(Clone)]
struct AppObject {
    record: AppRecord,
}

impl AppObject {
    fn new(record: AppRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "App")]
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

    async fn auth_config(&self) -> JsonValue {
        JsonValue(self.record.auth_config_json.clone())
    }

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

#[derive(Clone)]
struct AppCapabilityObject {
    record: AppCapabilityRecord,
}

impl AppCapabilityObject {
    fn new(record: AppCapabilityRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppCapability")]
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

#[derive(Clone)]
struct AppExternalConnectionObject {
    record: AppConnectionRecord,
}

impl AppExternalConnectionObject {
    fn new(record: AppConnectionRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppConnection")]
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

    async fn connection(&self) -> JsonValue {
        JsonValue(self.record.connection_json.clone())
    }
}

#[derive(Clone)]
struct AppSecretObject {
    record: AppSecretRecord,
}

impl AppSecretObject {
    fn new(record: AppSecretRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "AppSecret")]
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

#[derive(Clone)]
struct ScheduleJobObject {
    record: ScheduleJobRecord,
}

impl ScheduleJobObject {
    fn new(record: ScheduleJobRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ScheduleJob")]
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

    async fn payload(&self) -> JsonValue {
        JsonValue(self.record.payload.clone())
    }

    async fn headers(&self) -> JsonValue {
        JsonValue(self.record.headers.clone())
    }

    async fn schedule_spec(&self) -> JsonValue {
        JsonValue(self.record.schedule_spec.clone())
    }

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

#[derive(Clone)]
struct ScheduleJobRunObject {
    record: ScheduleJobRunRecord,
}

impl ScheduleJobRunObject {
    fn new(record: ScheduleJobRunRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "ScheduleJobRun")]
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

#[derive(Clone)]
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

#[Object(name = "Task")]
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

    async fn review(&self) -> ReviewStateObject {
        ReviewStateObject {
            submitted_at: self.record.review.submitted_at.clone(),
            approved_at: self.record.review.approved_at.clone(),
            changes_requested_at: self.record.review.changes_requested_at.clone(),
        }
    }

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
struct ReviewStateObject {
    submitted_at: Option<String>,
    approved_at: Option<String>,
    changes_requested_at: Option<String>,
}

#[derive(Clone)]
struct TaskCommentObject {
    record: borg_taskgraph::CommentRecord,
}

impl TaskCommentObject {
    fn new(record: borg_taskgraph::CommentRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "TaskComment")]
impl TaskCommentObject {
    async fn id(&self) -> &str {
        &self.record.id
    }

    async fn task_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.task_uri).ok().map(UriScalar)
    }

    async fn author_actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.author_actor_id).ok().map(UriScalar)
    }

    async fn body(&self) -> &str {
        &self.record.body
    }

    async fn created_at(&self) -> &str {
        &self.record.created_at
    }
}

#[derive(Clone)]
struct TaskEventObject {
    record: EventRecord,
}

impl TaskEventObject {
    fn new(record: EventRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "TaskEvent")]
impl TaskEventObject {
    async fn id(&self) -> &str {
        &self.record.id
    }

    async fn task_uri(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.task_uri).ok().map(UriScalar)
    }

    async fn actor_id(&self) -> Option<UriScalar> {
        Uri::parse(&self.record.actor_id).ok().map(UriScalar)
    }

    #[graphql(name = "type")]
    async fn event_type(&self) -> &str {
        &self.record.event_type
    }

    async fn data(&self) -> GqlResult<TaskEventDataObject> {
        let value =
            serde_json::to_value(&self.record.data).map_err(|err| map_anyhow(err.into()))?;
        Ok(TaskEventDataObject::from_json(
            self.record.event_type.clone(),
            value,
        ))
    }

    async fn created_at(&self) -> &str {
        &self.record.created_at
    }
}

#[derive(SimpleObject, Clone)]
struct TaskEventDataObject {
    kind: String,
    assignee_actor_id: Option<String>,
    reviewer_actor_id: Option<String>,
    parent_uri: Option<String>,
    title: Option<String>,
    description: Option<String>,
    definition_of_done: Option<String>,
    old_assignee_actor_id: Option<String>,
    new_assignee_actor_id: Option<String>,
    labels: Option<Vec<String>>,
    blocked_by: Option<String>,
    duplicate_of: Option<String>,
    reference: Option<String>,
    status: Option<String>,
    submitted_at: Option<String>,
    approved_at: Option<String>,
    changes_requested_at: Option<String>,
    return_to: Option<String>,
    note: Option<String>,
    subtask_count: Option<i64>,
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

#[derive(Clone)]
struct MemoryEntityObject {
    record: Entity,
}

impl MemoryEntityObject {
    fn new(record: Entity) -> Self {
        Self { record }
    }
}

#[Object(name = "MemoryEntity")]
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
struct MemoryPropertyObject {
    key: String,
    value: MemoryValueObject,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
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
struct MemoryValueObject {
    kind: MemoryValueKind,
    text: Option<String>,
    integer: Option<i64>,
    float: Option<f64>,
    boolean: Option<bool>,
    bytes_base64: Option<String>,
    reference: Option<UriScalar>,
    date: Option<String>,
    datetime: Option<String>,
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

#[derive(Clone)]
struct MemoryFactObject {
    record: FactRecord,
}

impl MemoryFactObject {
    fn new(record: FactRecord) -> Self {
        Self { record }
    }
}

#[Object(name = "MemoryFact")]
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
enum Node {
    Actor(ActorObject),
    Port(PortObject),
    Provider(ProviderObject),
    App(AppObject),
    Task(TaskObject),
    MemoryEntity(MemoryEntityObject),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum ActorStatusValue {
    Running,
    Paused,
    Disabled,
    Error,
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
enum AppStatusValue {
    Active,
    Inactive,
    Disabled,
    Archived,
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
            _ => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum AppCapabilityStatusValue {
    Active,
    Inactive,
    Disabled,
    Deprecated,
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
            _ => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum AppConnectionStatusValue {
    Connected,
    Disconnected,
    Pending,
    Revoked,
    Error,
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
            _ => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum ScheduleJobStatusValue {
    Active,
    Paused,
    Cancelled,
    Completed,
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
            _ => "unknown",
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum TaskStatusValue {
    Pending,
    Doing,
    Review,
    Done,
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
struct RunActorChatResult {
    ok: bool,
    message: String,
}

#[derive(SimpleObject)]
struct RunPortHttpResult {
    ok: bool,
    message: String,
}

mod resolvers;
