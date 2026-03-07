import { TypedDocumentNode as DocumentNode } from "@graphql-typed-document-node/core";
export type Maybe<T> = T | null;
export type InputMaybe<T> = Maybe<T>;
export type Exact<T extends { [key: string]: unknown }> = {
  [K in keyof T]: T[K];
};
export type MakeOptional<T, K extends keyof T> = Omit<T, K> & {
  [SubKey in K]?: Maybe<T[SubKey]>;
};
export type MakeMaybe<T, K extends keyof T> = Omit<T, K> & {
  [SubKey in K]: Maybe<T[SubKey]>;
};
export type MakeEmpty<
  T extends { [key: string]: unknown },
  K extends keyof T,
> = { [_ in K]?: never };
export type Incremental<T> =
  | T
  | {
      [P in keyof T]?: P extends " $fragmentName" | "__typename" ? T[P] : never;
    };
/** All built-in and custom scalars, mapped to their actual values */
export type Scalars = {
  ID: { input: string; output: string };
  String: { input: string; output: string };
  Boolean: { input: boolean; output: boolean };
  Int: { input: number; output: number };
  Float: { input: number; output: number };
  /**
   * Implement the DateTime<Utc> scalar
   *
   * The input/output is a string in RFC3339 format.
   */
  DateTime: { input: string; output: string };
  JsonValue: { input: unknown; output: unknown };
  Uri: { input: string; output: string };
};

/**
 * Runtime actor definition.
 *
 * An actor is a named, long-lived Borg worker/persona with its own message
 * history.
 *
 * Usage notes:
 * - Represents a runnable actor spec (`borg:actor:*`).
 * - Use `messages` for runtime timeline views.
 *
 * Example:
 * ```graphql
 * { actor(id: "borg:actor:planner") { id name status } }
 * ```
 */
export type Actor = Node & {
  __typename?: "Actor";
  createdAt: Scalars["DateTime"]["output"];
  defaultProviderId?: Maybe<Scalars["String"]["output"]>;
  /** Stable actor URI. */
  id: Scalars["Uri"]["output"];
  /**
   * Actor history messages.
   *
   * Usage notes:
   * - Backed by actor mailbox activity.
   *
   * Example:
   * ```graphql
   * { actor(id: "borg:actor:planner") { messages(first: 5) { edges { node { id messageType text } } } } }
   * ```
   */
  messages: ActorMessageConnection;
  model?: Maybe<Scalars["String"]["output"]>;
  name: Scalars["String"]["output"];
  status: ActorStatusValue;
  systemPrompt: Scalars["String"]["output"];
  updatedAt: Scalars["DateTime"]["output"];
};

/**
 * Runtime actor definition.
 *
 * An actor is a named, long-lived Borg worker/persona with its own message
 * history.
 *
 * Usage notes:
 * - Represents a runnable actor spec (`borg:actor:*`).
 * - Use `messages` for runtime timeline views.
 *
 * Example:
 * ```graphql
 * { actor(id: "borg:actor:planner") { id name status } }
 * ```
 */
export type ActorMessagesArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type ActorConnection = {
  __typename?: "ActorConnection";
  /** Returned edges for the current page. */
  edges: Array<ActorEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type ActorEdge = {
  __typename?: "ActorEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: Actor;
};

/**
 * Persisted timeline entry inside an actor history.
 *
 * Actor messages represent user inputs, assistant outputs, tool activity, and
 * lifecycle events as ordered records.
 *
 * Usage notes:
 * - Prefer `messageType`, `role`, and `text` over deprecated `payload`.
 * - `id` is the stable timeline message URI.
 *
 * Example:
 * ```graphql
 * { actor(id: "borg:actor:planner") { messages(first: 5) { edges { node { id messageType role text } } } } }
 * ```
 */
export type ActorMessage = {
  __typename?: "ActorMessage";
  actorId: Scalars["Uri"]["output"];
  createdAt: Scalars["DateTime"]["output"];
  id: Scalars["Uri"]["output"];
  messageType: Scalars["String"]["output"];
  /** @deprecated Legacy JSON payload. Prefer typed fields (`messageType`, `role`, `text`). */
  payload: Scalars["JsonValue"]["output"];
  role?: Maybe<Scalars["String"]["output"]>;
  text?: Maybe<Scalars["String"]["output"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type ActorMessageConnection = {
  __typename?: "ActorMessageConnection";
  /** Returned edges for the current page. */
  edges: Array<ActorMessageEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type ActorMessageEdge = {
  __typename?: "ActorMessageEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: ActorMessage;
};

/** High-level classification for UI routing of actor notifications. */
export enum ActorNotificationKind {
  /** Actor lifecycle or system event message. */
  ActorEvent = "ACTOR_EVENT",
  /** Assistant-authored response message. */
  AssistantReply = "ASSISTANT_REPLY",
  /** Fallback generic message classification. */
  Message = "MESSAGE",
  /** Tool call or tool result activity. */
  ToolActivity = "TOOL_ACTIVITY",
}

/**
 * Notification payload projected from an actor message.
 *
 * Usage notes:
 * - Notifications are fully typed and include the underlying `actorMessage`.
 * - `kind` is derived from `messageType`/`role`.
 */
export type ActorNotificationObject = {
  __typename?: "ActorNotificationObject";
  /** Actor URI this notification belongs to. */
  actorId: Scalars["Uri"]["output"];
  /** Full underlying message object. */
  actorMessage: ActorMessage;
  /** Source message timestamp. */
  createdAt: Scalars["DateTime"]["output"];
  /** Stable notification identifier (`actorId:messageId`). */
  id: Scalars["String"]["output"];
  /** Kind classification for UI routing and badges. */
  kind: ActorNotificationKind;
  /** Source message URI. */
  messageId: Scalars["Uri"]["output"];
  /** Source message type. */
  messageType: Scalars["String"]["output"];
  /** Source role, when available. */
  role?: Maybe<Scalars["String"]["output"]>;
  /** Source text content, when available. */
  text?: Maybe<Scalars["String"]["output"]>;
  /** Short user-facing title. */
  title: Scalars["String"]["output"];
};

/** Lifecycle states for actor records. */
export enum ActorStatusValue {
  /** Actor is disabled and should not run. */
  Disabled = "DISABLED",
  /** Actor encountered a terminal/error state. */
  Error = "ERROR",
  /** Actor is intentionally paused. */
  Paused = "PAUSED",
  /** Actor can receive and execute new work. */
  Running = "RUNNING",
  /** Actor row contains an unrecognized status value. */
  Unknown = "UNKNOWN",
}

/**
 * External app integration definition.
 *
 * Apps represent integrated systems (for example GitHub) and own capabilities,
 * account connections, and secret scopes.
 *
 * Usage notes:
 * - Parent object for capabilities, external connections, and secrets.
 * - `authConfig` is transitional JSON; prefer typed auth fields as they are added.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { id slug capabilities(first: 5) { edges { node { name mode } } } } }
 * ```
 */
export type App = Node & {
  __typename?: "App";
  /** @deprecated Legacy JSON auth config. Prefer typed auth fields over time. */
  authConfig: Scalars["JsonValue"]["output"];
  authStrategy: Scalars["String"]["output"];
  availableSecrets: Array<Scalars["String"]["output"]>;
  builtIn: Scalars["Boolean"]["output"];
  /**
   * Capability definitions available on this app.
   *
   * Example:
   * ```graphql
   * { appBySlug(slug: "github") { capabilities(first: 20) { edges { node { id name mode } } } } }
   * ```
   */
  capabilities: AppCapabilityConnection;
  /**
   * External account connections for this app.
   *
   * Example:
   * ```graphql
   * { appBySlug(slug: "github") { connections(first: 20) { edges { node { id status ownerUserId } } } } }
   * ```
   */
  connections: AppExternalConnectionConnection;
  createdAt: Scalars["DateTime"]["output"];
  description: Scalars["String"]["output"];
  id: Scalars["Uri"]["output"];
  name: Scalars["String"]["output"];
  /**
   * Secrets available to this app (optionally filtered by connection).
   *
   * Example:
   * ```graphql
   * { appBySlug(slug: "github") { secrets(first: 20) { edges { node { id key kind } } } } }
   * ```
   */
  secrets: AppSecretConnection;
  slug: Scalars["String"]["output"];
  source: Scalars["String"]["output"];
  status: AppStatusValue;
  updatedAt: Scalars["DateTime"]["output"];
};

/**
 * External app integration definition.
 *
 * Apps represent integrated systems (for example GitHub) and own capabilities,
 * account connections, and secret scopes.
 *
 * Usage notes:
 * - Parent object for capabilities, external connections, and secrets.
 * - `authConfig` is transitional JSON; prefer typed auth fields as they are added.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { id slug capabilities(first: 5) { edges { node { name mode } } } } }
 * ```
 */
export type AppCapabilitiesArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * External app integration definition.
 *
 * Apps represent integrated systems (for example GitHub) and own capabilities,
 * account connections, and secret scopes.
 *
 * Usage notes:
 * - Parent object for capabilities, external connections, and secrets.
 * - `authConfig` is transitional JSON; prefer typed auth fields as they are added.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { id slug capabilities(first: 5) { edges { node { name mode } } } } }
 * ```
 */
export type AppConnectionsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * External app integration definition.
 *
 * Apps represent integrated systems (for example GitHub) and own capabilities,
 * account connections, and secret scopes.
 *
 * Usage notes:
 * - Parent object for capabilities, external connections, and secrets.
 * - `authConfig` is transitional JSON; prefer typed auth fields as they are added.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { id slug capabilities(first: 5) { edges { node { name mode } } } } }
 * ```
 */
export type AppSecretsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  connectionId?: InputMaybe<Scalars["Uri"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * App operation that can be invoked by runtime/tooling.
 *
 * Usage notes:
 * - Capability rows describe app operations exposed to runtime/LLMs.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { capabilities(first: 5) { edges { node { id name mode status } } } } }
 * ```
 */
export type AppCapability = {
  __typename?: "AppCapability";
  appId: Scalars["Uri"]["output"];
  createdAt: Scalars["DateTime"]["output"];
  hint: Scalars["String"]["output"];
  id: Scalars["Uri"]["output"];
  instructions: Scalars["String"]["output"];
  mode: Scalars["String"]["output"];
  name: Scalars["String"]["output"];
  status: AppCapabilityStatusValue;
  updatedAt: Scalars["DateTime"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type AppCapabilityConnection = {
  __typename?: "AppCapabilityConnection";
  /** Returned edges for the current page. */
  edges: Array<AppCapabilityEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type AppCapabilityEdge = {
  __typename?: "AppCapabilityEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: AppCapability;
};

/** Lifecycle states for app capability rows. */
export enum AppCapabilityStatusValue {
  /** Capability is enabled and invokable. */
  Active = "ACTIVE",
  /** Capability is deprecated and retained for compatibility. */
  Deprecated = "DEPRECATED",
  /** Capability is disabled. */
  Disabled = "DISABLED",
  /** Capability exists but is currently inactive. */
  Inactive = "INACTIVE",
  /** Capability row contains an unrecognized status value. */
  Unknown = "UNKNOWN",
}

/**
 * Linked external account for an app integration.
 *
 * Usage notes:
 * - Represents one user/account connection to an app integration.
 * - `connection` is transitional JSON metadata.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { connections(first: 5) { edges { node { id ownerUserId status } } } } }
 * ```
 */
export type AppConnection = {
  __typename?: "AppConnection";
  appId: Scalars["Uri"]["output"];
  /** @deprecated Legacy JSON connection payload. Prefer typed fields over time. */
  connection: Scalars["JsonValue"]["output"];
  createdAt: Scalars["DateTime"]["output"];
  externalUserId?: Maybe<Scalars["String"]["output"]>;
  id: Scalars["Uri"]["output"];
  ownerUserId?: Maybe<Scalars["Uri"]["output"]>;
  providerAccountId?: Maybe<Scalars["String"]["output"]>;
  status: AppConnectionStatusValue;
  updatedAt: Scalars["DateTime"]["output"];
};

/** Connection states for external app accounts. */
export enum AppConnectionStatusValue {
  /** Connection is healthy and ready for use. */
  Connected = "CONNECTED",
  /** Connection exists but is not currently authenticated. */
  Disconnected = "DISCONNECTED",
  /** Connection is in an error state. */
  Error = "ERROR",
  /** Connection setup is in progress. */
  Pending = "PENDING",
  /** Connection exists but is intentionally revoked. */
  Revoked = "REVOKED",
  /** Connection row contains an unrecognized status value. */
  Unknown = "UNKNOWN",
}

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type AppEdge = {
  __typename?: "AppEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: App;
};

/** Relay-style page container for cursor-based list traversal. */
export type AppExternalConnectionConnection = {
  __typename?: "AppExternalConnectionConnection";
  /** Returned edges for the current page. */
  edges: Array<AppExternalConnectionEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type AppExternalConnectionEdge = {
  __typename?: "AppExternalConnectionEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: AppConnection;
};

/** Relay-style page container for cursor-based list traversal. */
export type AppListConnection = {
  __typename?: "AppListConnection";
  /** Returned edges for the current page. */
  edges: Array<AppEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/**
 * Secret row scoped to an app or a specific app connection.
 *
 * Usage notes:
 * - Secrets can be global per-app or scoped to `connectionId`.
 *
 * Example:
 * ```graphql
 * { appBySlug(slug: "github") { secrets(first: 5) { edges { node { id key kind connectionId } } } } }
 * ```
 */
export type AppSecret = {
  __typename?: "AppSecret";
  appId: Scalars["Uri"]["output"];
  connectionId?: Maybe<Scalars["Uri"]["output"]>;
  createdAt: Scalars["DateTime"]["output"];
  id: Scalars["Uri"]["output"];
  key: Scalars["String"]["output"];
  kind: Scalars["String"]["output"];
  updatedAt: Scalars["DateTime"]["output"];
  value: Scalars["String"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type AppSecretConnection = {
  __typename?: "AppSecretConnection";
  /** Returned edges for the current page. */
  edges: Array<AppSecretEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type AppSecretEdge = {
  __typename?: "AppSecretEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: AppSecret;
};

/** Lifecycle states for app integration definitions. */
export enum AppStatusValue {
  /** App integration is enabled and available. */
  Active = "ACTIVE",
  /** App integration is archived and preserved for history. */
  Archived = "ARCHIVED",
  /** App integration is disabled. */
  Disabled = "DISABLED",
  /** App integration exists but is currently inactive. */
  Inactive = "INACTIVE",
  /** App row contains an unrecognized status value. */
  Unknown = "UNKNOWN",
}

/**
 * Creates a new schedule scheduler job definition.
 *
 * Example:
 * `{ jobId: "daily-digest", kind: "cron", actorId: "borg:actor:planner" }`
 */
export type CreateScheduleJobInputGql = {
  /** Actor URI executed by this job. */
  actorId: Scalars["Uri"]["input"];
  /** Optional job headers (transitional JSON). */
  headers?: InputMaybe<Scalars["JsonValue"]["input"]>;
  /** Stable job identifier. */
  jobId: Scalars["String"]["input"];
  /** Scheduler kind (`cron`, ...). */
  kind: Scalars["String"]["input"];
  /** Message envelope type. */
  messageType: Scalars["String"]["input"];
  /** Optional explicit first run timestamp (RFC3339 string). */
  nextRunAt?: InputMaybe<Scalars["String"]["input"]>;
  /** Job payload (transitional JSON). */
  payload: Scalars["JsonValue"]["input"];
  /** Schedule specification (transitional JSON). */
  scheduleSpec: Scalars["JsonValue"]["input"];
};

/**
 * Creates a new durable taskgraph task.
 *
 * Example:
 * `{ actorId: "borg:actor:creator", creatorActorId: "borg:actor:creator", assigneeActorId: "borg:actor:assignee", title: "Ship docs" }`
 */
export type CreateTaskInputGql = {
  /** Actor URI authoring the task create event. */
  actorId: Scalars["Uri"]["input"];
  /** Assignee actor URI. */
  assigneeActorId: Scalars["Uri"]["input"];
  /** Task URIs that block this task. */
  blockedBy?: Array<Scalars["Uri"]["input"]>;
  /** Creator actor URI. */
  creatorActorId: Scalars["Uri"]["input"];
  /** Completion criteria. */
  definitionOfDone?: Scalars["String"]["input"];
  /** Task description/body. */
  description?: Scalars["String"]["input"];
  /** User-defined labels. */
  labels?: Array<Scalars["String"]["input"]>;
  /** Parent task URI when creating a subtask. */
  parentUri?: InputMaybe<Scalars["Uri"]["input"]>;
  /** Related entity/task URIs. */
  references?: Array<Scalars["Uri"]["input"]>;
  /** Short task title. */
  title: Scalars["String"]["input"];
};

/**
 * Canonical entity node in Borg long-term memory.
 *
 * Usage notes:
 * - Entity vertex with typed property map (`props`).
 * - `facts` exposes fact rows that target this entity.
 *
 * Example:
 * ```graphql
 * { memoryEntity(id: "borg:entity:alice") { id label props { key value { kind text } } } }
 * ```
 */
export type MemoryEntity = Node & {
  __typename?: "MemoryEntity";
  createdAt: Scalars["DateTime"]["output"];
  entityType: Scalars["Uri"]["output"];
  /**
   * Facts that target this memory entity.
   *
   * Example:
   * ```graphql
   * { memoryEntity(id: "borg:entity:alice") { facts(first: 20) { edges { node { field value { kind text } } } } } }
   * ```
   */
  facts: MemoryFactConnection;
  id: Scalars["Uri"]["output"];
  label: Scalars["String"]["output"];
  /**
   * Property map on the entity, projected as typed key/value pairs.
   *
   * Example:
   * ```graphql
   * { memoryEntity(id: "borg:entity:alice") { props { key value { kind text reference } } } }
   * ```
   */
  props: Array<MemoryPropertyObject>;
  updatedAt: Scalars["DateTime"]["output"];
};

/**
 * Canonical entity node in Borg long-term memory.
 *
 * Usage notes:
 * - Entity vertex with typed property map (`props`).
 * - `facts` exposes fact rows that target this entity.
 *
 * Example:
 * ```graphql
 * { memoryEntity(id: "borg:entity:alice") { id label props { key value { kind text } } } }
 * ```
 */
export type MemoryEntityFactsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  fieldId?: InputMaybe<Scalars["Uri"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
  includeRetracted?: InputMaybe<Scalars["Boolean"]["input"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type MemoryEntityConnection = {
  __typename?: "MemoryEntityConnection";
  /** Returned edges for the current page. */
  edges: Array<MemoryEntityEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type MemoryEntityEdge = {
  __typename?: "MemoryEntityEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: MemoryEntity;
};

/**
 * Immutable fact assertion row in long-term memory storage.
 *
 * Usage notes:
 * - Immutable fact rows with typed value projection.
 * - Use `isRetracted` for audit-aware consumers.
 *
 * Example:
 * ```graphql
 * { memoryFacts(first: 5) { edges { node { id field arity value { kind text } isRetracted } } } }
 * ```
 */
export type MemoryFact = {
  __typename?: "MemoryFact";
  /** Fact field cardinality (`ONE` or `MANY`). */
  arity: MemoryFactArity;
  /** Entity URI that this fact targets. */
  entity: Scalars["Uri"]["output"];
  /** Field URI for this fact. */
  field: Scalars["Uri"]["output"];
  /** Fact URI. */
  id: Scalars["Uri"]["output"];
  /** Whether the fact has been retracted. */
  isRetracted: Scalars["Boolean"]["output"];
  /** Source URI that asserted this fact. */
  source: Scalars["Uri"]["output"];
  /** Timestamp when the fact was stated. */
  statedAt: Scalars["DateTime"]["output"];
  /** Transaction URI that wrote this fact row. */
  txId: Scalars["Uri"]["output"];
  /** Typed value projection for the fact payload. */
  value: MemoryValueObject;
};

/** Cardinality contract for a memory fact field. */
export enum MemoryFactArity {
  Many = "MANY",
  One = "ONE",
}

/** Relay-style page container for cursor-based list traversal. */
export type MemoryFactConnection = {
  __typename?: "MemoryFactConnection";
  /** Returned edges for the current page. */
  edges: Array<MemoryFactEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type MemoryFactEdge = {
  __typename?: "MemoryFactEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: MemoryFact;
};

/** One typed property entry on a memory entity. */
export type MemoryPropertyObject = {
  __typename?: "MemoryPropertyObject";
  /** Property key name. */
  key: Scalars["String"]["output"];
  /** Typed property value. */
  value: MemoryValueObject;
};

/** Type discriminator for normalized memory values. */
export enum MemoryValueKind {
  Boolean = "BOOLEAN",
  Bytes = "BYTES",
  Date = "DATE",
  DateTime = "DATE_TIME",
  Float = "FLOAT",
  Integer = "INTEGER",
  List = "LIST",
  Ref = "REF",
  Text = "TEXT",
}

/**
 * Normalized typed value used by memory properties and facts.
 *
 * Usage notes:
 * - Inspect `kind` first, then read the matching typed field.
 * - Non-matching fields remain `null`.
 */
export type MemoryValueObject = {
  __typename?: "MemoryValueObject";
  /** Boolean value when `kind = BOOLEAN`. */
  boolean?: Maybe<Scalars["Boolean"]["output"]>;
  /** Base64-url encoded bytes when `kind = BYTES`. */
  bytesBase64?: Maybe<Scalars["String"]["output"]>;
  /** Date value (`YYYY-MM-DD`) when `kind = DATE`. */
  date?: Maybe<Scalars["String"]["output"]>;
  /** Date-time value (RFC3339) when `kind = DATE_TIME`. */
  datetime?: Maybe<Scalars["String"]["output"]>;
  /** Floating-point value when `kind = FLOAT`. */
  float?: Maybe<Scalars["Float"]["output"]>;
  /** Integer value when `kind = INTEGER`. */
  integer?: Maybe<Scalars["Int"]["output"]>;
  /** Value discriminator. */
  kind: MemoryValueKind;
  /** Nested typed values when `kind = LIST`. */
  list?: Maybe<Array<MemoryValueObject>>;
  /** URI reference when `kind = REF`. */
  reference?: Maybe<Scalars["Uri"]["output"]>;
  /** String value when `kind = TEXT`. */
  text?: Maybe<Scalars["String"]["output"]>;
};

export type ModelObject = {
  __typename?: "ModelObject";
  name: Scalars["String"]["output"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRoot = {
  __typename?: "MutationRoot";
  /**
   * Cancels a schedule job.
   *
   * Example:
   * ```graphql
   * mutation { cancelScheduleJob(jobId: "daily-standup") }
   * ```
   */
  cancelScheduleJob: Scalars["Boolean"]["output"];
  /**
   * Creates a schedule job.
   *
   * Example:
   * ```graphql
   * mutation($actor: Uri!) {
   * createScheduleJob(input: {
   * jobId: "daily-standup"
   * kind: "cron"
   * actorId: $actor
   * messageType: "user"
   * payload: { text: "Daily standup" }
   * scheduleSpec: { cron: "0 9 * * 1-5" }
   * }) { id status nextRunAt }
   * }
   * ```
   */
  createScheduleJob: ScheduleJob;
  /**
   * Creates a task in the taskgraph store.
   *
   * Example:
   * ```graphql
   * mutation($actor: Uri!, $creator: Uri!, $assignee: Uri!) {
   * createTask(input: {
   * actorId: $actor
   * creatorActorId: $creator
   * assigneeActorId: $assignee
   * title: "Ship borg-gql docs"
   * description: "Document all schema entrypoints"
   * }) { id title status }
   * }
   * ```
   */
  createTask: Task;
  /**
   * Deletes an actor by URI.
   *
   * Example:
   * ```graphql
   * mutation($id: Uri!) { deleteActor(id: $id) }
   * ```
   */
  deleteActor: Scalars["Boolean"]["output"];
  /**
   * Deletes a provider and associated usage summary.
   *
   * Example:
   * ```graphql
   * mutation { deleteProvider(provider: "openrouter") }
   * ```
   */
  deleteProvider: Scalars["Boolean"]["output"];
  /**
   * Pauses an active schedule job.
   *
   * Example:
   * ```graphql
   * mutation { pauseScheduleJob(jobId: "daily-standup") }
   * ```
   */
  pauseScheduleJob: Scalars["Boolean"]["output"];
  /**
   * Resumes a paused schedule job.
   *
   * Example:
   * ```graphql
   * mutation { resumeScheduleJob(jobId: "daily-standup") }
   * ```
   */
  resumeScheduleJob: Scalars["Boolean"]["output"];
  /**
   * Placeholder runtime wrapper; enabled after `borg-api` integration.
   *
   * Usage notes:
   * - Currently returns `BAD_REQUEST`.
   * - Keep frontend contracts ready for upcoming runtime integration.
   *
   * Example:
   * ```graphql
   * mutation($actor: Uri!, $user: Uri!) {
   * runActorChat(input: {
   * actorId: $actor
   * userId: $user
   * text: "Summarize pending tasks"
   * }) { ok message }
   * }
   * ```
   */
  runActorChat: RunActorChatResult;
  /**
   * Placeholder runtime wrapper; enabled after `borg-api` integration.
   *
   * Usage notes:
   * - Currently returns `BAD_REQUEST`.
   * - Expected to mirror `POST /ports/http` behavior in a later phase.
   *
   * Example:
   * ```graphql
   * mutation($user: Uri!) {
   * runPortHttp(input: { userId: $user, text: "Hello" }) { ok message }
   * }
   * ```
   */
  runPortHttp: RunPortHttpResult;
  /**
   * Moves a task to a new allowed status.
   *
   * Usage notes:
   * - Auth constraints follow taskgraph rules (assignee/reviewer checks).
   *
   * Example:
   * ```graphql
   * mutation($task: Uri!, $actor: Uri!) {
   * setTaskStatus(input: { taskId: $task, actorId: $actor, status: DOING }) {
   * id
   * status
   * }
   * }
   * ```
   */
  setTaskStatus: Task;
  /**
   * Updates mutable schedule job fields.
   *
   * Example:
   * ```graphql
   * mutation {
   * updateScheduleJob(
   * jobId: "daily-standup",
   * patch: { scheduleSpec: { cron: "0 10 * * 1-5" } }
   * ) { id status scheduleSpec }
   * }
   * ```
   */
  updateScheduleJob: ScheduleJob;
  /**
   * Updates mutable task text fields.
   *
   * Example:
   * ```graphql
   * mutation($task: Uri!, $actor: Uri!) {
   * updateTask(input: {
   * taskId: $task
   * actorId: $actor
   * title: "Updated title"
   * }) { id title updatedAt }
   * }
   * ```
   */
  updateTask: Task;
  /**
   * Creates or updates an actor.
   *
   * Example:
   * ```graphql
   * mutation($id: Uri!) {
   * upsertActor(input: {
   * id: $id
   * name: "Planner"
   * systemPrompt: "You plan work."
   * status: RUNNING
   * }) { id name status }
   * }
   * ```
   */
  upsertActor: Actor;
  /**
   * Creates or updates an app.
   *
   * Example:
   * ```graphql
   * mutation($id: Uri!) {
   * upsertApp(input: {
   * id: $id
   * name: "GitHub"
   * slug: "github"
   * description: "GitHub integration"
   * status: ACTIVE
   * builtIn: false
   * source: "custom"
   * authStrategy: "oauth2"
   * availableSecrets: ["GITHUB_TOKEN"]
   * }) { id slug status }
   * }
   * ```
   */
  upsertApp: App;
  /**
   * Creates or updates an app capability.
   *
   * Example:
   * ```graphql
   * mutation($app: Uri!, $cap: Uri!) {
   * upsertAppCapability(input: {
   * appId: $app
   * capabilityId: $cap
   * name: "issues.list"
   * hint: "List GitHub issues"
   * mode: "READ"
   * instructions: "Use filters when possible"
   * status: ACTIVE
   * }) { id name status }
   * }
   * ```
   */
  upsertAppCapability: AppCapability;
  /**
   * Creates or updates an app connection.
   *
   * Example:
   * ```graphql
   * mutation($app: Uri!, $conn: Uri!, $owner: Uri) {
   * upsertAppConnection(input: {
   * appId: $app
   * connectionId: $conn
   * ownerUserId: $owner
   * status: CONNECTED
   * }) { id appId status }
   * }
   * ```
   */
  upsertAppConnection: AppConnection;
  /**
   * Creates or updates an app secret.
   *
   * Example:
   * ```graphql
   * mutation($app: Uri!, $secret: Uri!) {
   * upsertAppSecret(input: {
   * appId: $app
   * secretId: $secret
   * key: "GITHUB_TOKEN"
   * value: "..."
   * kind: "token"
   * }) { id key kind }
   * }
   * ```
   */
  upsertAppSecret: AppSecret;
  /**
   * Creates or updates a port.
   *
   * Usage notes:
   * - `assignedActorId` is mirrored into `settings.actor_id` for compatibility.
   * - `settings` must be a JSON object when provided.
   *
   * Example:
   * ```graphql
   * mutation {
   * upsertPort(input: {
   * name: "http"
   * provider: "custom"
   * enabled: true
   * allowsGuests: true
   * }) { id name enabled allowsGuests }
   * }
   * ```
   */
  upsertPort: Port;
  /**
   * Creates or updates a port/actor binding.
   *
   * Usage notes:
   * - Pass `actorId: null` to clear the actor binding.
   *
   * Example:
   * ```graphql
   * mutation($key: Uri!, $actor: Uri!) {
   * upsertPortActorBinding(input: {
   * portName: "telegram"
   * conversationKey: $key
   * actorId: $actor
   * }) { portName conversationKey actorId }
   * }
   * ```
   */
  upsertPortActorBinding: PortActorBinding;
  /**
   * Creates or updates a port/actor binding.
   *
   * Example:
   * ```graphql
   * mutation($actor: Uri!, $key: Uri!) {
   * upsertPortBinding(input: {
   * portName: "telegram"
   * conversationKey: $key
   * actorId: $actor
   * }) { portName conversationKey actorId }
   * }
   * ```
   */
  upsertPortBinding: PortBinding;
  /**
   * Creates or updates a provider.
   *
   * Example:
   * ```graphql
   * mutation {
   * upsertProvider(input: {
   * provider: "openrouter"
   * providerKind: "openrouter"
   * apiKey: "sk-***"
   * enabled: true
   * defaultTextModel: "openai/gpt-4.1-mini"
   * }) { provider providerKind enabled }
   * }
   * ```
   */
  upsertProvider: Provider;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootCancelScheduleJobArgs = {
  jobId: Scalars["String"]["input"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootCreateScheduleJobArgs = {
  input: CreateScheduleJobInputGql;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootCreateTaskArgs = {
  input: CreateTaskInputGql;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootDeleteActorArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootDeleteProviderArgs = {
  provider: Scalars["String"]["input"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootPauseScheduleJobArgs = {
  jobId: Scalars["String"]["input"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootResumeScheduleJobArgs = {
  jobId: Scalars["String"]["input"];
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootRunActorChatArgs = {
  input: RunActorChatInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootRunPortHttpArgs = {
  input: RunPortHttpInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootSetTaskStatusArgs = {
  input: SetTaskStatusInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpdateScheduleJobArgs = {
  jobId: Scalars["String"]["input"];
  patch: UpdateScheduleJobInputGql;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpdateTaskArgs = {
  input: UpdateTaskInputGql;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertActorArgs = {
  input: UpsertActorInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertAppArgs = {
  input: UpsertAppInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertAppCapabilityArgs = {
  input: UpsertAppCapabilityInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertAppConnectionArgs = {
  input: UpsertAppConnectionInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertAppSecretArgs = {
  input: UpsertAppSecretInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertPortArgs = {
  input: UpsertPortInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertPortActorBindingArgs = {
  input: UpsertPortActorBindingInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertPortBindingArgs = {
  input: UpsertPortBindingInput;
};

/**
 * Root mutation entrypoint for control-plane and task/memory writes.
 *
 * Usage notes:
 * - Mutations return the written object whenever possible.
 * - URI arguments are strictly validated by the `Uri` scalar.
 * - Runtime wrapper mutations are intentionally stubbed until `borg-api` integration.
 *
 * Example:
 * ```graphql
 * mutation {
 * upsertProvider(input: { provider: "openai", providerKind: "openai", enabled: true }) {
 * provider
 * enabled
 * }
 * }
 * ```
 */
export type MutationRootUpsertProviderArgs = {
  input: UpsertProviderInput;
};

/**
 * Unified node interface for cross-entity graph traversal.
 *
 * Example:
 * ```graphql
 * query($id: Uri!) {
 * node(id: $id) {
 * id
 * ... on Actor { name status }
 * }
 * }
 * ```
 */
export type Node = {
  id: Scalars["Uri"]["output"];
};

/**
 * Cursor pagination metadata shared by all `*Connection` types.
 *
 * Usage notes:
 * - Reuse `endCursor` as the next request's `after` argument.
 * - Stop paginating when `hasNextPage` is `false`.
 *
 * Example:
 * ```graphql
 * {
 * actors(first: 20) {
 * pageInfo { hasNextPage endCursor }
 * }
 * }
 * ```
 */
export type PageInfo = {
  __typename?: "PageInfo";
  /** Opaque cursor for fetching the next page. */
  endCursor?: Maybe<Scalars["String"]["output"]>;
  /** Whether more pages are available after `endCursor`. */
  hasNextPage: Scalars["Boolean"]["output"];
};

/**
 * External transport adapter configuration.
 *
 * Ports model how Borg receives/sends traffic (for example HTTP, Telegram) and
 * how incoming conversations map into actors.
 *
 * Usage notes:
 * - Ports bind external channels (`http`, `telegram`, ...) to actor routing.
 * - `bindings` and `actorBindings` expose live routing maps.
 *
 * Example:
 * ```graphql
 * { port(name: "telegram") { id provider enabled bindings(first: 5) { edges { node { conversationKey actorId } } } } }
 * ```
 */
export type Port = Node & {
  __typename?: "Port";
  activeBindings: Scalars["Int"]["output"];
  /**
   * Actor binding rows for this port.
   *
   * Example:
   * ```graphql
   * { port(name: "telegram") { actorBindings(first: 10) { edges { node { conversationKey actorId } } } } }
   * ```
   */
  actorBindings: PortActorBindingConnection;
  allowsGuests: Scalars["Boolean"]["output"];
  /**
   * Optional default actor explicitly assigned to this port.
   *
   * Example:
   * ```graphql
   * { port(name: "telegram") { assignedActor { id name status } } }
   * ```
   */
  assignedActor?: Maybe<Actor>;
  assignedActorId?: Maybe<Scalars["Uri"]["output"]>;
  /**
   * Actor binding rows for this port.
   *
   * Example:
   * ```graphql
   * { port(name: "telegram") { bindings(first: 10) { edges { node { conversationKey actorId } } } } }
   * ```
   */
  bindings: PortBindingConnection;
  enabled: Scalars["Boolean"]["output"];
  id: Scalars["Uri"]["output"];
  name: Scalars["String"]["output"];
  provider: Scalars["String"]["output"];
  /** @deprecated Legacy JSON settings. Prefer typed fields over time. */
  settings: Scalars["JsonValue"]["output"];
  updatedAt?: Maybe<Scalars["DateTime"]["output"]>;
};

/**
 * External transport adapter configuration.
 *
 * Ports model how Borg receives/sends traffic (for example HTTP, Telegram) and
 * how incoming conversations map into actors.
 *
 * Usage notes:
 * - Ports bind external channels (`http`, `telegram`, ...) to actor routing.
 * - `bindings` and `actorBindings` expose live routing maps.
 *
 * Example:
 * ```graphql
 * { port(name: "telegram") { id provider enabled bindings(first: 5) { edges { node { conversationKey actorId } } } } }
 * ```
 */
export type PortActorBindingsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * External transport adapter configuration.
 *
 * Ports model how Borg receives/sends traffic (for example HTTP, Telegram) and
 * how incoming conversations map into actors.
 *
 * Usage notes:
 * - Ports bind external channels (`http`, `telegram`, ...) to actor routing.
 * - `bindings` and `actorBindings` expose live routing maps.
 *
 * Example:
 * ```graphql
 * { port(name: "telegram") { id provider enabled bindings(first: 5) { edges { node { conversationKey actorId } } } } }
 * ```
 */
export type PortBindingsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Conversation-specific actor override edge.
 *
 * This row pins a conversation to a specific actor independently from the
 * default port actor.
 *
 * Usage notes:
 * - Stores explicit actor overrides for a conversation key.
 *
 * Example:
 * ```graphql
 * { port(name: "telegram") { actorBindings(first: 5) { edges { node { conversationKey actorId } } } } }
 * ```
 */
export type PortActorBinding = {
  __typename?: "PortActorBinding";
  /** Expanded actor object for this actor binding row. */
  actor?: Maybe<Actor>;
  actorId?: Maybe<Scalars["Uri"]["output"]>;
  conversationKey: Scalars["Uri"]["output"];
  id: Scalars["String"]["output"];
  portName: Scalars["String"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type PortActorBindingConnection = {
  __typename?: "PortActorBindingConnection";
  /** Returned edges for the current page. */
  edges: Array<PortActorBindingEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type PortActorBindingEdge = {
  __typename?: "PortActorBindingEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: PortActorBinding;
};

/**
 * Conversation routing edge from a port to an actor.
 *
 * Usage notes:
 * - Canonical ingress-actor routing row.
 * - `actor` resolves the bound actor object.
 *
 * Example:
 * ```graphql
 * { port(name: "telegram") { bindings(first: 5) { edges { node { conversationKey actorId actor { id } } } } } }
 * ```
 */
export type PortBinding = {
  __typename?: "PortBinding";
  /**
   * Actor bound to this conversation key, if any.
   *
   * Example:
   * ```graphql
   * { port(name: "telegram") { bindings(first: 1) { edges { node { actor { id name } } } } } }
   * ```
   */
  actor?: Maybe<Actor>;
  actorId: Scalars["Uri"]["output"];
  conversationKey: Scalars["Uri"]["output"];
  id: Scalars["String"]["output"];
  portName: Scalars["String"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type PortBindingConnection = {
  __typename?: "PortBindingConnection";
  /** Returned edges for the current page. */
  edges: Array<PortBindingEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type PortBindingEdge = {
  __typename?: "PortBindingEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: PortBinding;
};

/** Relay-style page container for cursor-based list traversal. */
export type PortConnection = {
  __typename?: "PortConnection";
  /** Returned edges for the current page. */
  edges: Array<PortEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type PortEdge = {
  __typename?: "PortEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: Port;
};

/**
 * LLM provider configuration and usage counters.
 *
 * Provider rows hold credentials, default models, and operational metadata for
 * model routing.
 *
 * Usage notes:
 * - `provider` is the configuration key.
 * - `providerKind` maps to adapter family when needed.
 *
 * Example:
 * ```graphql
 * { provider(provider: "openai") { provider providerKind enabled defaultTextModel models { name } defaultModel { name } } }
 * ```
 */
export type Provider = Node & {
  __typename?: "Provider";
  apiKey: Scalars["String"]["output"];
  baseUrl?: Maybe<Scalars["String"]["output"]>;
  createdAt: Scalars["DateTime"]["output"];
  defaultAudioModel?: Maybe<Scalars["String"]["output"]>;
  defaultModel: ModelObject;
  defaultTextModel?: Maybe<Scalars["String"]["output"]>;
  enabled: Scalars["Boolean"]["output"];
  id: Scalars["Uri"]["output"];
  lastUsed?: Maybe<Scalars["DateTime"]["output"]>;
  models: Array<ModelObject>;
  provider: Scalars["String"]["output"];
  providerKind: Scalars["String"]["output"];
  tokensUsed: Scalars["Int"]["output"];
  updatedAt: Scalars["DateTime"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type ProviderConnection = {
  __typename?: "ProviderConnection";
  /** Returned edges for the current page. */
  edges: Array<ProviderEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type ProviderEdge = {
  __typename?: "ProviderEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: Provider;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRoot = {
  __typename?: "QueryRoot";
  /**
   * Fetches one actor by URI.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) { actor(id: $id) { id name status } }
   * ```
   */
  actor?: Maybe<Actor>;
  /**
   * Lists actors ordered by most-recent update.
   *
   * Usage notes:
   * - `first` defaults to 25 and is capped server-side.
   * - Pass the previous `endCursor` into `after` to paginate.
   *
   * Example:
   * ```graphql
   * query {
   * actors(first: 10) {
   * edges { cursor node { id name } }
   * pageInfo { hasNextPage endCursor }
   * }
   * }
   * ```
   */
  actors: ActorConnection;
  /**
   * Fetches one app by URI.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) { app(id: $id) { id name slug status } }
   * ```
   */
  app?: Maybe<App>;
  /**
   * Fetches one app by slug.
   *
   * Example:
   * ```graphql
   * query { appBySlug(slug: "github") { id name capabilities(first: 5) { edges { node { name } } } } }
   * ```
   */
  appBySlug?: Maybe<App>;
  /**
   * Lists apps available in Borg.
   *
   * Example:
   * ```graphql
   * query {
   * apps(first: 25) {
   * edges { node { id slug authStrategy availableSecrets } }
   * }
   * }
   * ```
   */
  apps: AppListConnection;
  /**
   * Searches memory entities by free text plus optional namespace/kind filters.
   *
   * Example:
   * ```graphql
   * query {
   * memoryEntities(queryText: "alice", kind: "person", first: 10) {
   * edges { node { id label } }
   * }
   * }
   * ```
   */
  memoryEntities: MemoryEntityConnection;
  /**
   * Fetches one memory entity by URI.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) { memoryEntity(id: $id) { id label props { key value { kind text } } } }
   * ```
   */
  memoryEntity?: Maybe<MemoryEntity>;
  /**
   * Fetches one fact row by fact URI string.
   *
   * Example:
   * ```graphql
   * query($id: String!) { memoryFact(id: $id) { id arity value { kind text } } }
   * ```
   */
  memoryFact?: Maybe<MemoryFact>;
  /**
   * Lists fact rows with optional entity/field filters.
   *
   * Usage notes:
   * - Set `includeRetracted: true` for audit/replay tooling.
   *
   * Example:
   * ```graphql
   * query($entity: Uri!) {
   * memoryFacts(entityId: $entity, first: 20) {
   * edges { node { id field value { kind text reference } } }
   * }
   * }
   * ```
   */
  memoryFacts: MemoryFactConnection;
  /**
   * Fetches a single graph node by URI and resolves the concrete runtime type.
   *
   * Usage notes:
   * - Works for actor/port/provider/app/task/memory entities.
   * - Use inline fragments to read type-specific fields.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) {
   * node(id: $id) {
   * id
   * ... on Actor { name status }
   * }
   * }
   * ```
   */
  node?: Maybe<Node>;
  /**
   * Fetches one port by canonical port name (for example `http`, `telegram`).
   *
   * Example:
   * ```graphql
   * query { port(name: "http") { id name provider enabled } }
   * ```
   */
  port?: Maybe<Port>;
  /**
   * Fetches one port by URI.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) { portById(id: $id) { id name allowsGuests } }
   * ```
   */
  portById?: Maybe<Port>;
  /**
   * Lists ports ordered by activity.
   *
   * Usage notes:
   * - Includes `activeBindings` and binding relations for routing debugging.
   *
   * Example:
   * ```graphql
   * query {
   * ports(first: 20) {
   * edges { node { name provider activeBindings } }
   * }
   * }
   * ```
   */
  ports: PortConnection;
  /**
   * Fetches one provider by provider key.
   *
   * Example:
   * ```graphql
   * query { provider(provider: "openai") { id provider providerKind enabled } }
   * ```
   */
  provider?: Maybe<Provider>;
  /**
   * Lists configured model providers.
   *
   * Example:
   * ```graphql
   * query {
   * providers(first: 10) {
   * edges { node { provider providerKind defaultTextModel tokensUsed } }
   * }
   * }
   * ```
   */
  providers: ProviderConnection;
  /**
   * Fetches one schedule job by `jobId`.
   *
   * Example:
   * ```graphql
   * query { scheduleJob(jobId: "daily-digest") { id status nextRunAt } }
   * ```
   */
  scheduleJob?: Maybe<ScheduleJob>;
  /**
   * Lists schedule jobs with optional status filtering.
   *
   * Example:
   * ```graphql
   * query {
   * scheduleJobs(first: 20, status: ACTIVE) {
   * edges { node { id kind status runs(first: 5) { edges { node { id firedAt } } } } }
   * }
   * }
   * ```
   */
  scheduleJobs: ScheduleJobConnection;
  /**
   * Fetches one task by URI.
   *
   * Example:
   * ```graphql
   * query($id: Uri!) {
   * task(id: $id) {
   * id title status
   * comments(first: 10) { edges { node { id body } } }
   * }
   * }
   * ```
   */
  task?: Maybe<Task>;
  /**
   * Lists top-level taskgraph tasks.
   *
   * Usage notes:
   * - Cursor format follows taskgraph ordering (`createdAt`, `id`).
   * - Traverse children via `Task.children`.
   *
   * Example:
   * ```graphql
   * query {
   * tasks(first: 15) {
   * edges { node { id title status parentUri } }
   * }
   * }
   * ```
   */
  tasks: TaskConnection;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootActorArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootActorsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootAppArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootAppBySlugArgs = {
  slug: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootAppsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootMemoryEntitiesArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
  kind?: InputMaybe<Scalars["String"]["input"]>;
  ns?: InputMaybe<Scalars["String"]["input"]>;
  queryText: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootMemoryEntityArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootMemoryFactArgs = {
  id: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootMemoryFactsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  entityId?: InputMaybe<Scalars["Uri"]["input"]>;
  fieldId?: InputMaybe<Scalars["Uri"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
  includeRetracted?: InputMaybe<Scalars["Boolean"]["input"]>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootNodeArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootPortArgs = {
  name: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootPortByIdArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootPortsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootProviderArgs = {
  provider: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootProvidersArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootScheduleJobArgs = {
  jobId: Scalars["String"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootScheduleJobsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
  status?: InputMaybe<ScheduleJobStatusValue>;
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootTaskArgs = {
  id: Scalars["Uri"]["input"];
};

/**
 * Root query entrypoint for the Borg entity graph.
 *
 * Usage notes:
 * - Use `node(id: Uri!)` for generic cross-entity lookup.
 * - Use typed roots (`actor`, `tasks`, `apps`, etc.) for stronger discoverability.
 * - Connection fields use cursor pagination via `first` + `after`.
 * - For full operation recipes, see `crates/borg-gql/SCHEMA_USAGE.md`.
 *
 * Example:
 * ```graphql
 * query {
 * actors(first: 5) {
 * edges { node { id name status } }
 * }
 * }
 * ```
 */
export type QueryRootTasksArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Task review lifecycle timestamps.
 *
 * Usage notes:
 * - A field is `null` until that transition happened.
 */
export type ReviewStateObject = {
  __typename?: "ReviewStateObject";
  /** Timestamp when task was approved. */
  approvedAt?: Maybe<Scalars["String"]["output"]>;
  /** Timestamp when changes were requested. */
  changesRequestedAt?: Maybe<Scalars["String"]["output"]>;
  /** Timestamp when task entered review. */
  submittedAt?: Maybe<Scalars["String"]["output"]>;
};

/**
 * Future runtime input shape for direct actor chat execution.
 *
 * Usage notes:
 * - Reserved for future runtime integration.
 */
export type RunActorChatInput = {
  /** Actor URI to execute. */
  actorId: Scalars["Uri"]["input"];
  /** User text to send. */
  text: Scalars["String"]["input"];
  /** User URI authoring the request. */
  userId: Scalars["Uri"]["input"];
};

/** Future runtime response contract for direct actor chat execution. */
export type RunActorChatResult = {
  __typename?: "RunActorChatResult";
  /** Human-readable status message. */
  message: Scalars["String"]["output"];
  /** Whether the runtime call succeeded. */
  ok: Scalars["Boolean"]["output"];
};

/**
 * Future runtime input shape mirroring HTTP port execution.
 *
 * Usage notes:
 * - Reserved for future runtime integration.
 */
export type RunPortHttpInput = {
  /** Optional explicit actor URI override. */
  actorId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** User text to send. */
  text: Scalars["String"]["input"];
  /** User URI authoring the request. */
  userId: Scalars["Uri"]["input"];
};

/** Future runtime response contract for HTTP-port style execution. */
export type RunPortHttpResult = {
  __typename?: "RunPortHttpResult";
  /** Human-readable status message. */
  message: Scalars["String"]["output"];
  /** Whether the runtime call succeeded. */
  ok: Scalars["Boolean"]["output"];
};

/**
 * Durable scheduler job definition for automated actor execution.
 *
 * Usage notes:
 * - Defines recurring/queued actor execution plans.
 * - Use `runs` to inspect execution history.
 *
 * Example:
 * ```graphql
 * { scheduleJob(jobId: "daily-digest") { id kind status nextRunAt runs(first: 5) { edges { node { id firedAt } } } } }
 * ```
 */
export type ScheduleJob = {
  __typename?: "ScheduleJob";
  createdAt: Scalars["DateTime"]["output"];
  /** @deprecated Legacy JSON headers. Prefer typed fields over time. */
  headers: Scalars["JsonValue"]["output"];
  id: Scalars["String"]["output"];
  kind: Scalars["String"]["output"];
  lastRunAt?: Maybe<Scalars["DateTime"]["output"]>;
  messageType: Scalars["String"]["output"];
  nextRunAt?: Maybe<Scalars["DateTime"]["output"]>;
  /** @deprecated Legacy JSON payload. Prefer typed fields over time. */
  payload: Scalars["JsonValue"]["output"];
  /**
   * Historical run rows for this schedule job.
   *
   * Example:
   * ```graphql
   * { scheduleJob(jobId: "daily") { runs(first: 10) { edges { node { id firedAt messageId } } } } }
   * ```
   */
  runs: ScheduleJobRunConnection;
  /** @deprecated Legacy JSON schedule spec. Prefer typed schedule fields over time. */
  scheduleSpec: Scalars["JsonValue"]["output"];
  status: ScheduleJobStatusValue;
  targetActorId?: Maybe<Scalars["Uri"]["output"]>;
  updatedAt: Scalars["DateTime"]["output"];
};

/**
 * Durable scheduler job definition for automated actor execution.
 *
 * Usage notes:
 * - Defines recurring/queued actor execution plans.
 * - Use `runs` to inspect execution history.
 *
 * Example:
 * ```graphql
 * { scheduleJob(jobId: "daily-digest") { id kind status nextRunAt runs(first: 5) { edges { node { id firedAt } } } } }
 * ```
 */
export type ScheduleJobRunsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type ScheduleJobConnection = {
  __typename?: "ScheduleJobConnection";
  /** Returned edges for the current page. */
  edges: Array<ScheduleJobEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type ScheduleJobEdge = {
  __typename?: "ScheduleJobEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: ScheduleJob;
};

/**
 * Immutable execution record emitted when a schedule job fires.
 *
 * Usage notes:
 * - Immutable execution row emitted by schedule runtime.
 *
 * Example:
 * ```graphql
 * { scheduleJob(jobId: "daily-digest") { runs(first: 5) { edges { node { id scheduledFor firedAt messageId } } } } }
 * ```
 */
export type ScheduleJobRun = {
  __typename?: "ScheduleJobRun";
  createdAt: Scalars["DateTime"]["output"];
  firedAt: Scalars["DateTime"]["output"];
  id: Scalars["String"]["output"];
  jobId: Scalars["String"]["output"];
  messageId: Scalars["String"]["output"];
  scheduledFor: Scalars["DateTime"]["output"];
  targetActorId?: Maybe<Scalars["Uri"]["output"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type ScheduleJobRunConnection = {
  __typename?: "ScheduleJobRunConnection";
  /** Returned edges for the current page. */
  edges: Array<ScheduleJobRunEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type ScheduleJobRunEdge = {
  __typename?: "ScheduleJobRunEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: ScheduleJobRun;
};

/** Runtime lifecycle states for scheduler jobs. */
export enum ScheduleJobStatusValue {
  /** Job is active and eligible to run. */
  Active = "ACTIVE",
  /** Job has been cancelled. */
  Cancelled = "CANCELLED",
  /** Job completed and will not run again. */
  Completed = "COMPLETED",
  /** Job is paused and should not be scheduled. */
  Paused = "PAUSED",
  /** Job row contains an unrecognized status value. */
  Unknown = "UNKNOWN",
}

/**
 * Requests a task status transition under taskgraph rules.
 *
 * Example:
 * `{ taskId: "borg:task:t1", actorId: "borg:actor:assignee", status: DOING }`
 */
export type SetTaskStatusInput = {
  /** Actor URI authoring the status transition. */
  actorId: Scalars["Uri"]["input"];
  /** Target status value. */
  status: TaskStatusValue;
  /** Task URI to transition. */
  taskId: Scalars["Uri"]["input"];
};

/**
 * Root subscription entrypoint for real-time Borg streams.
 *
 * Usage notes:
 * - Subscription transport is expected to run over WebSockets (`graphql-transport-ws`).
 * - Use `actorChat` for actor timeline streaming.
 * - Use `actorNotifications` for notification-friendly filtered events.
 *
 * Example:
 * ```graphql
 * subscription($actor: Uri!) {
 * actorChat(actorId: $actor) {
 * id
 * messageType
 * role
 * text
 * }
 * }
 * ```
 */
export type SubscriptionRoot = {
  __typename?: "SubscriptionRoot";
  /**
   * Streams new messages from an actor timeline as they are appended.
   *
   * Usage notes:
   * - When `afterMessageId` is omitted, the stream starts from the first message.
   * - Provide `afterMessageId` to replay from a known point.
   * - `pollIntervalMs` is clamped to safe server bounds.
   *
   * Example:
   * ```graphql
   * subscription($actor: Uri!, $after: Uri) {
   * actorChat(actorId: $actor, afterMessageId: $after, pollIntervalMs: 500) {
   * id
   * messageType
   * role
   * text
   * }
   * }
   * ```
   */
  actorChat: ActorMessage;
  /**
   * Streams actor notifications derived from new timeline messages.
   *
   * Usage notes:
   * - By default, user-authored messages are filtered out.
   * - Set `includeUserMessages: true` to receive all roles.
   *
   * Example:
   * ```graphql
   * subscription($actor: Uri!) {
   * actorNotifications(actorId: $actor) {
   * id
   * kind
   * title
   * text
   * actorMessage { id messageType role }
   * }
   * }
   * ```
   */
  actorNotifications: ActorNotificationObject;
};

/**
 * Root subscription entrypoint for real-time Borg streams.
 *
 * Usage notes:
 * - Subscription transport is expected to run over WebSockets (`graphql-transport-ws`).
 * - Use `actorChat` for actor timeline streaming.
 * - Use `actorNotifications` for notification-friendly filtered events.
 *
 * Example:
 * ```graphql
 * subscription($actor: Uri!) {
 * actorChat(actorId: $actor) {
 * id
 * messageType
 * role
 * text
 * }
 * }
 * ```
 */
export type SubscriptionRootActorChatArgs = {
  actorId: Scalars["Uri"]["input"];
  afterMessageId?: InputMaybe<Scalars["Uri"]["input"]>;
  pollIntervalMs?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Root subscription entrypoint for real-time Borg streams.
 *
 * Usage notes:
 * - Subscription transport is expected to run over WebSockets (`graphql-transport-ws`).
 * - Use `actorChat` for actor timeline streaming.
 * - Use `actorNotifications` for notification-friendly filtered events.
 *
 * Example:
 * ```graphql
 * subscription($actor: Uri!) {
 * actorChat(actorId: $actor) {
 * id
 * messageType
 * role
 * text
 * }
 * }
 * ```
 */
export type SubscriptionRootActorNotificationsArgs = {
  actorId: Scalars["Uri"]["input"];
  afterMessageId?: InputMaybe<Scalars["Uri"]["input"]>;
  includeUserMessages?: InputMaybe<Scalars["Boolean"]["input"]>;
  pollIntervalMs?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Durable taskgraph work item.
 *
 * Tasks carry assignment, dependencies, audit history, and review lifecycle
 * state for explicit multi-step work.
 *
 * Usage notes:
 * - Task is the core work item in durable taskgraph storage.
 * - `parent`/`children` and `comments`/`events` provide graph + audit traversal.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { id title status children(first: 5) { edges { node { id title } } } } }
 * ```
 */
export type Task = Node & {
  __typename?: "Task";
  assigneeActorId: Scalars["String"]["output"];
  blockedBy: Array<Scalars["Uri"]["output"]>;
  /**
   * Child task rows directly under this task.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t-parent") { children(first: 20) { edges { node { id title status } } } } }
   * ```
   */
  children: TaskConnection;
  /**
   * Comment timeline for this task.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t1") { comments(first: 20) { edges { node { id body authorActorId } } } } }
   * ```
   */
  comments: TaskCommentConnection;
  createdAt: Scalars["String"]["output"];
  definitionOfDone: Scalars["String"]["output"];
  description: Scalars["String"]["output"];
  duplicateOf?: Maybe<Scalars["Uri"]["output"]>;
  /**
   * Event timeline for this task.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t1") { events(first: 20) { edges { node { id type createdAt } } } } }
   * ```
   */
  events: TaskEventConnection;
  id: Scalars["Uri"]["output"];
  labels: Array<Scalars["String"]["output"]>;
  /**
   * Parent task object, if this task is a subtask.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t-child") { parent { id title status } } }
   * ```
   */
  parent?: Maybe<Task>;
  parentUri?: Maybe<Scalars["Uri"]["output"]>;
  references: Array<Scalars["Uri"]["output"]>;
  /**
   * Review state timestamps for this task.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t1") { review { submittedAt approvedAt changesRequestedAt } } }
   * ```
   */
  review: ReviewStateObject;
  reviewerActorId: Scalars["String"]["output"];
  status: TaskStatusValue;
  title: Scalars["String"]["output"];
  updatedAt: Scalars["String"]["output"];
};

/**
 * Durable taskgraph work item.
 *
 * Tasks carry assignment, dependencies, audit history, and review lifecycle
 * state for explicit multi-step work.
 *
 * Usage notes:
 * - Task is the core work item in durable taskgraph storage.
 * - `parent`/`children` and `comments`/`events` provide graph + audit traversal.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { id title status children(first: 5) { edges { node { id title } } } } }
 * ```
 */
export type TaskChildrenArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Durable taskgraph work item.
 *
 * Tasks carry assignment, dependencies, audit history, and review lifecycle
 * state for explicit multi-step work.
 *
 * Usage notes:
 * - Task is the core work item in durable taskgraph storage.
 * - `parent`/`children` and `comments`/`events` provide graph + audit traversal.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { id title status children(first: 5) { edges { node { id title } } } } }
 * ```
 */
export type TaskCommentsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Durable taskgraph work item.
 *
 * Tasks carry assignment, dependencies, audit history, and review lifecycle
 * state for explicit multi-step work.
 *
 * Usage notes:
 * - Task is the core work item in durable taskgraph storage.
 * - `parent`/`children` and `comments`/`events` provide graph + audit traversal.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { id title status children(first: 5) { edges { node { id title } } } } }
 * ```
 */
export type TaskEventsArgs = {
  after?: InputMaybe<Scalars["String"]["input"]>;
  first?: InputMaybe<Scalars["Int"]["input"]>;
};

/**
 * Human/agent comment attached to a task timeline.
 *
 * Usage notes:
 * - Comment timeline entries attached to a task.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { comments(first: 5) { edges { node { id body createdAt } } } } }
 * ```
 */
export type TaskComment = {
  __typename?: "TaskComment";
  /** Actor URI that authored this comment. */
  authorActorId?: Maybe<Scalars["Uri"]["output"]>;
  /** Comment body text. */
  body: Scalars["String"]["output"];
  /** Comment creation timestamp. */
  createdAt: Scalars["String"]["output"];
  /** Stable comment identifier. */
  id: Scalars["String"]["output"];
  /** Task URI that this comment belongs to. */
  taskUri?: Maybe<Scalars["Uri"]["output"]>;
};

/** Relay-style page container for cursor-based list traversal. */
export type TaskCommentConnection = {
  __typename?: "TaskCommentConnection";
  /** Returned edges for the current page. */
  edges: Array<TaskCommentEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type TaskCommentEdge = {
  __typename?: "TaskCommentEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: TaskComment;
};

/** Relay-style page container for cursor-based list traversal. */
export type TaskConnection = {
  __typename?: "TaskConnection";
  /** Returned edges for the current page. */
  edges: Array<TaskEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type TaskEdge = {
  __typename?: "TaskEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: Task;
};

/**
 * Structured audit event emitted by taskgraph transitions.
 *
 * Usage notes:
 * - Event timeline entries with typed payload projection in `data`.
 *
 * Example:
 * ```graphql
 * { task(id: "borg:task:t1") { events(first: 5) { edges { node { id type data { kind status } } } } } }
 * ```
 */
export type TaskEvent = {
  __typename?: "TaskEvent";
  /** Actor URI that triggered the event. */
  actorId?: Maybe<Scalars["Uri"]["output"]>;
  /** Event creation timestamp. */
  createdAt: Scalars["String"]["output"];
  /**
   * Event payload projected into typed optional fields.
   *
   * Usage notes:
   * - Read `kind` first, then use matching payload fields.
   *
   * Example:
   * ```graphql
   * { task(id: "borg:task:t1") { events(first: 1) { edges { node { type data { kind status note } } } } } }
   * ```
   */
  data: TaskEventDataObject;
  /** Stable event identifier. */
  id: Scalars["String"]["output"];
  /** Task URI that emitted this event. */
  taskUri?: Maybe<Scalars["Uri"]["output"]>;
  type: Scalars["String"]["output"];
};

/** Relay-style page container for cursor-based list traversal. */
export type TaskEventConnection = {
  __typename?: "TaskEventConnection";
  /** Returned edges for the current page. */
  edges: Array<TaskEventEdge>;
  /** Pagination state for the current page. */
  pageInfo: PageInfo;
};

/**
 * Typed projection of task event payload details.
 *
 * Usage notes:
 * - `kind` indicates which subset of optional fields is populated.
 * - Consumers should branch on `kind` before reading event-specific fields.
 */
export type TaskEventDataObject = {
  __typename?: "TaskEventDataObject";
  /** Review approval timestamp when relevant. */
  approvedAt?: Maybe<Scalars["String"]["output"]>;
  /** New assignee actor ID when relevant. */
  assigneeActorId?: Maybe<Scalars["String"]["output"]>;
  /** Blocking dependency URI when relevant. */
  blockedBy?: Maybe<Scalars["String"]["output"]>;
  /** Review changes-requested timestamp when relevant. */
  changesRequestedAt?: Maybe<Scalars["String"]["output"]>;
  /** Created comment ID when relevant. */
  commentId?: Maybe<Scalars["String"]["output"]>;
  /** Updated definition-of-done when relevant. */
  definitionOfDone?: Maybe<Scalars["String"]["output"]>;
  /** Updated description when relevant. */
  description?: Maybe<Scalars["String"]["output"]>;
  /** Duplicate-of URI when relevant. */
  duplicateOf?: Maybe<Scalars["String"]["output"]>;
  /** Event type/kind copied from the event row. */
  kind: Scalars["String"]["output"];
  /** Label list payload when relevant. */
  labels?: Maybe<Array<Scalars["String"]["output"]>>;
  /** Replacement assignee actor ID when relevant. */
  newAssigneeActorId?: Maybe<Scalars["String"]["output"]>;
  /** Free-text note when relevant. */
  note?: Maybe<Scalars["String"]["output"]>;
  /** Previous assignee actor ID when relevant. */
  oldAssigneeActorId?: Maybe<Scalars["String"]["output"]>;
  /** Parent task URI when relevant. */
  parentUri?: Maybe<Scalars["String"]["output"]>;
  /** Reference URI when relevant. */
  reference?: Maybe<Scalars["String"]["output"]>;
  /** Return status destination when relevant. */
  returnTo?: Maybe<Scalars["String"]["output"]>;
  /** Reviewer actor ID when relevant. */
  reviewerActorId?: Maybe<Scalars["String"]["output"]>;
  /** Status value when relevant. */
  status?: Maybe<Scalars["String"]["output"]>;
  /** Review submit timestamp when relevant. */
  submittedAt?: Maybe<Scalars["String"]["output"]>;
  /** Subtask count delta when relevant. */
  subtaskCount?: Maybe<Scalars["Int"]["output"]>;
  /** Updated title when relevant. */
  title?: Maybe<Scalars["String"]["output"]>;
};

/** Relay-style edge carrying one node plus cursor for forward pagination. */
export type TaskEventEdge = {
  __typename?: "TaskEventEdge";
  /** Opaque edge cursor to pass back into `after`. */
  cursor: Scalars["String"]["output"];
  /** Materialized node for this edge. */
  node: TaskEvent;
};

/** TaskGraph status values accepted by `setTaskStatus`. */
export enum TaskStatusValue {
  /** Explicitly discarded. */
  Discarded = "DISCARDED",
  /** Work in progress. */
  Doing = "DOING",
  /** Completed successfully. */
  Done = "DONE",
  /** Newly created task. */
  Pending = "PENDING",
  /** Awaiting review. */
  Review = "REVIEW",
}

/**
 * Patches mutable fields on an existing schedule scheduler job.
 *
 * Usage notes:
 * - Only pass fields that need to change.
 * - `nextRunAt` set to `null` clears the existing schedule override.
 */
export type UpdateScheduleJobInputGql = {
  /** New target actor URI. */
  actorId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** New headers (transitional JSON). */
  headers?: InputMaybe<Scalars["JsonValue"]["input"]>;
  /** New scheduler kind. */
  kind?: InputMaybe<Scalars["String"]["input"]>;
  /** New message type. */
  messageType?: InputMaybe<Scalars["String"]["input"]>;
  /** New next run timestamp (RFC3339 string). */
  nextRunAt?: InputMaybe<Scalars["String"]["input"]>;
  /** New payload (transitional JSON). */
  payload?: InputMaybe<Scalars["JsonValue"]["input"]>;
  /** New schedule spec (transitional JSON). */
  scheduleSpec?: InputMaybe<Scalars["JsonValue"]["input"]>;
};

/**
 * Patches editable text fields on an existing taskgraph task.
 *
 * Usage notes:
 * - This mutation only patches text fields.
 * - Leave fields as `null` to keep previous values.
 */
export type UpdateTaskInputGql = {
  /** Actor URI authoring the update event. */
  actorId: Scalars["Uri"]["input"];
  /** Optional new definition of done. */
  definitionOfDone?: InputMaybe<Scalars["String"]["input"]>;
  /** Optional new description. */
  description?: InputMaybe<Scalars["String"]["input"]>;
  /** Task URI to patch. */
  taskId: Scalars["Uri"]["input"];
  /** Optional new title. */
  title?: InputMaybe<Scalars["String"]["input"]>;
};

/**
 * Creates or updates an actor definition in Borg's control-plane graph.
 *
 * Example:
 * `{ id: "borg:actor:planner", name: "Planner", status: RUNNING }`
 */
export type UpsertActorInput = {
  /** Stable actor URI (`borg:actor:*`). */
  id: Scalars["Uri"]["input"];
  /** Optional runtime model id (for example `gpt-4.1`). */
  model?: InputMaybe<Scalars["String"]["input"]>;
  /** Human-readable actor name. */
  name: Scalars["String"]["input"];
  /** Actor lifecycle status (for example `RUNNING`). */
  status: ActorStatusValue;
  /** System prompt used when running this actor. */
  systemPrompt: Scalars["String"]["input"];
};

/**
 * Creates or updates a capability exposed by an app integration.
 *
 * Example:
 * `{ appId: "borg:app:github", capabilityId: "borg:capability:issues-list", name: "issues.list", mode: "READ" }`
 */
export type UpsertAppCapabilityInput = {
  /** Parent app URI. */
  appId: Scalars["Uri"]["input"];
  /** Capability URI. */
  capabilityId: Scalars["Uri"]["input"];
  /** Short hint for UI/LLM tooltips. */
  hint: Scalars["String"]["input"];
  /** Detailed execution instructions for this capability. */
  instructions: Scalars["String"]["input"];
  /** Capability mode (`READ`, `WRITE`, ...). */
  mode: Scalars["String"]["input"];
  /** Capability display name. */
  name: Scalars["String"]["input"];
  /** Capability lifecycle status. */
  status: AppCapabilityStatusValue;
};

/**
 * Creates or updates an external-account connection row for an app.
 *
 * Example:
 * `{ appId: "borg:app:github", connectionId: "borg:app-connection:octocat", status: CONNECTED }`
 */
export type UpsertAppConnectionInput = {
  /** Parent app URI. */
  appId: Scalars["Uri"]["input"];
  /** Transitional JSON metadata for this connection. */
  connection?: InputMaybe<Scalars["JsonValue"]["input"]>;
  /** Stable connection URI. */
  connectionId: Scalars["Uri"]["input"];
  /** External user/account identifier. */
  externalUserId?: InputMaybe<Scalars["String"]["input"]>;
  /** Owning user URI. */
  ownerUserId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** Provider account identifier. */
  providerAccountId?: InputMaybe<Scalars["String"]["input"]>;
  /** Connection lifecycle status. */
  status: AppConnectionStatusValue;
};

/**
 * Creates or updates an app integration definition.
 *
 * Example:
 * `{ id: "borg:app:github", name: "GitHub", slug: "github", status: ACTIVE, authStrategy: "oauth2" }`
 */
export type UpsertAppInput = {
  /** Transitional JSON auth config. */
  authConfig?: InputMaybe<Scalars["JsonValue"]["input"]>;
  /** Authentication strategy (`none`, `oauth2`, ...). */
  authStrategy: Scalars["String"]["input"];
  /** Secret keys this app expects to read. */
  availableSecrets?: Array<Scalars["String"]["input"]>;
  /** Whether this app is bundled by Borg. */
  builtIn?: Scalars["Boolean"]["input"];
  /** Description shown in clients/admin screens. */
  description: Scalars["String"]["input"];
  /** Stable app URI (`borg:app:*`). */
  id: Scalars["Uri"]["input"];
  /** Human-readable app name. */
  name: Scalars["String"]["input"];
  /** URL-safe app slug. */
  slug: Scalars["String"]["input"];
  /** App source origin (`builtin`, `custom`, ...). */
  source: Scalars["String"]["input"];
  /** App lifecycle status. */
  status: AppStatusValue;
};

/**
 * Creates or updates secret material attached to an app or app connection.
 *
 * Example:
 * `{ appId: "borg:app:github", secretId: "borg:app-secret:token", key: "GITHUB_TOKEN", kind: "token" }`
 */
export type UpsertAppSecretInput = {
  /** Parent app URI. */
  appId: Scalars["Uri"]["input"];
  /** Optional scoped connection URI. */
  connectionId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** Secret key name. */
  key: Scalars["String"]["input"];
  /** Secret kind (`token`, `password`, ...). */
  kind: Scalars["String"]["input"];
  /** Stable secret URI. */
  secretId: Scalars["Uri"]["input"];
  /** Secret value. */
  value: Scalars["String"]["input"];
};

/**
 * Creates or updates the conversation-to-actor override row for a port.
 *
 * Example:
 * `{ portName: "telegram", conversationKey: "borg:conversation:123", actorId: "borg:actor:planner" }`
 */
export type UpsertPortActorBindingInput = {
  /** Actor URI or `null` to clear binding. */
  actorId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** Conversation key used for routing. */
  conversationKey: Scalars["Uri"]["input"];
  /** Port name (`http`, `telegram`, ...). */
  portName: Scalars["String"]["input"];
};

/**
 * Creates or updates the conversation-to-actor routing row for a port.
 *
 * Example:
 * `{ portName: "telegram", conversationKey: "borg:conversation:123", actorId: "borg:actor:planner" }`
 */
export type UpsertPortBindingInput = {
  /** Target actor URI. */
  actorId: Scalars["Uri"]["input"];
  /** Stable conversation key for ingress routing. */
  conversationKey: Scalars["Uri"]["input"];
  /** Port name (`http`, `telegram`, ...). */
  portName: Scalars["String"]["input"];
};

/**
 * Creates or updates a runtime ingress/egress port configuration.
 *
 * Example:
 * `{ name: "telegram", provider: "telegram", enabled: true, allowsGuests: false }`
 */
export type UpsertPortInput = {
  /** Whether unauthenticated users are accepted. */
  allowsGuests: Scalars["Boolean"]["input"];
  /** Optional default actor for this port. */
  assignedActorId?: InputMaybe<Scalars["Uri"]["input"]>;
  /** Whether the port can ingest traffic. */
  enabled: Scalars["Boolean"]["input"];
  /** Port name (for example `http`, `telegram`). */
  name: Scalars["String"]["input"];
  /** Port provider/transport family. */
  provider: Scalars["String"]["input"];
  /** Optional JSON settings object. */
  settings?: InputMaybe<Scalars["JsonValue"]["input"]>;
};

/**
 * Creates or updates an LLM provider configuration entry.
 *
 * Example:
 * `{ provider: "openai", providerKind: "openai", enabled: true, defaultTextModel: "gpt-4.1-mini" }`
 */
export type UpsertProviderInput = {
  /** API key/token for this provider. */
  apiKey?: InputMaybe<Scalars["String"]["input"]>;
  /** Optional base URL override. */
  baseUrl?: InputMaybe<Scalars["String"]["input"]>;
  /** Preferred default model for audio/transcription. */
  defaultAudioModel?: InputMaybe<Scalars["String"]["input"]>;
  /** Preferred default model for text generation. */
  defaultTextModel?: InputMaybe<Scalars["String"]["input"]>;
  /** Enable or disable the provider. */
  enabled?: InputMaybe<Scalars["Boolean"]["input"]>;
  /** Provider key (`openai`, `openrouter`, ...). */
  provider: Scalars["String"]["input"];
  /** Provider family/kind. Defaults to `provider` when omitted. */
  providerKind?: InputMaybe<Scalars["String"]["input"]>;
};

export type OnboardingUpsertActorMutationVariables = Exact<{
  input: UpsertActorInput;
}>;

export type OnboardingUpsertActorMutation = {
  __typename?: "MutationRoot";
  upsertActor: { __typename?: "Actor"; id: string };
};

export type OnboardingUpsertPortMutationVariables = Exact<{
  input: UpsertPortInput;
}>;

export type OnboardingUpsertPortMutation = {
  __typename?: "MutationRoot";
  upsertPort: { __typename?: "Port"; id: string };
};

export type OnboardingPortBindingsByPortIdQueryVariables = Exact<{
  portId: Scalars["Uri"]["input"];
  first: Scalars["Int"]["input"];
}>;

export type OnboardingPortBindingsByPortIdQuery = {
  __typename?: "QueryRoot";
  portById?: {
    __typename?: "Port";
    bindings: {
      __typename?: "PortBindingConnection";
      edges: Array<{
        __typename?: "PortBindingEdge";
        node: {
          __typename?: "PortBinding";
          conversationKey: string;
          actorId: string;
          actor?: { __typename?: "Actor"; id: string } | null;
        };
      }>;
    };
  } | null;
};

export type OnboardingActorMessagesQueryVariables = Exact<{
  actorId: Scalars["Uri"]["input"];
  first: Scalars["Int"]["input"];
  after?: InputMaybe<Scalars["String"]["input"]>;
}>;

export type OnboardingActorMessagesQuery = {
  __typename?: "QueryRoot";
  actor?: {
    __typename?: "Actor";
    messages: {
      __typename?: "ActorMessageConnection";
      edges: Array<{
        __typename?: "ActorMessageEdge";
        node: {
          __typename?: "ActorMessage";
          id: string;
          actorId: string;
          createdAt: string;
          messageType: string;
          role?: string | null;
          text?: string | null;
          payload: unknown;
        };
      }>;
      pageInfo: {
        __typename?: "PageInfo";
        endCursor?: string | null;
        hasNextPage: boolean;
      };
    };
  } | null;
};

export type ProviderFieldsFragment = {
  __typename?: "Provider";
  provider: string;
  providerKind: string;
  apiKey: string;
  baseUrl?: string | null;
  enabled: boolean;
  tokensUsed: number;
  lastUsed?: string | null;
  defaultTextModel?: string | null;
  defaultAudioModel?: string | null;
  createdAt: string;
  updatedAt: string;
};

export type ProvidersListQueryVariables = Exact<{
  first: Scalars["Int"]["input"];
  after?: InputMaybe<Scalars["String"]["input"]>;
}>;

export type ProvidersListQuery = {
  __typename?: "QueryRoot";
  providers: {
    __typename?: "ProviderConnection";
    edges: Array<{
      __typename?: "ProviderEdge";
      node: {
        __typename?: "Provider";
        provider: string;
        providerKind: string;
        apiKey: string;
        baseUrl?: string | null;
        enabled: boolean;
        tokensUsed: number;
        lastUsed?: string | null;
        defaultTextModel?: string | null;
        defaultAudioModel?: string | null;
        createdAt: string;
        updatedAt: string;
      };
    }>;
    pageInfo: {
      __typename?: "PageInfo";
      endCursor?: string | null;
      hasNextPage: boolean;
    };
  };
};

export type ProviderByKeyQueryVariables = Exact<{
  provider: Scalars["String"]["input"];
}>;

export type ProviderByKeyQuery = {
  __typename?: "QueryRoot";
  provider?: {
    __typename?: "Provider";
    provider: string;
    providerKind: string;
    apiKey: string;
    baseUrl?: string | null;
    enabled: boolean;
    tokensUsed: number;
    lastUsed?: string | null;
    defaultTextModel?: string | null;
    defaultAudioModel?: string | null;
    createdAt: string;
    updatedAt: string;
  } | null;
};

export type UpsertProviderMutationVariables = Exact<{
  input: UpsertProviderInput;
}>;

export type UpsertProviderMutation = {
  __typename?: "MutationRoot";
  upsertProvider: {
    __typename?: "Provider";
    provider: string;
    providerKind: string;
    apiKey: string;
    baseUrl?: string | null;
    enabled: boolean;
    tokensUsed: number;
    lastUsed?: string | null;
    defaultTextModel?: string | null;
    defaultAudioModel?: string | null;
    createdAt: string;
    updatedAt: string;
  };
};

export type DeleteProviderMutationVariables = Exact<{
  provider: Scalars["String"]["input"];
}>;

export type DeleteProviderMutation = {
  __typename?: "MutationRoot";
  deleteProvider: boolean;
};

export const ProviderFieldsFragmentDoc = {
  kind: "Document",
  definitions: [
    {
      kind: "FragmentDefinition",
      name: { kind: "Name", value: "ProviderFields" },
      typeCondition: {
        kind: "NamedType",
        name: { kind: "Name", value: "Provider" },
      },
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          { kind: "Field", name: { kind: "Name", value: "provider" } },
          { kind: "Field", name: { kind: "Name", value: "providerKind" } },
          { kind: "Field", name: { kind: "Name", value: "apiKey" } },
          { kind: "Field", name: { kind: "Name", value: "baseUrl" } },
          { kind: "Field", name: { kind: "Name", value: "enabled" } },
          { kind: "Field", name: { kind: "Name", value: "tokensUsed" } },
          { kind: "Field", name: { kind: "Name", value: "lastUsed" } },
          { kind: "Field", name: { kind: "Name", value: "defaultTextModel" } },
          { kind: "Field", name: { kind: "Name", value: "defaultAudioModel" } },
          { kind: "Field", name: { kind: "Name", value: "createdAt" } },
          { kind: "Field", name: { kind: "Name", value: "updatedAt" } },
        ],
      },
    },
  ],
} as unknown as DocumentNode<ProviderFieldsFragment, unknown>;
export const OnboardingUpsertActorDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "mutation",
      name: { kind: "Name", value: "OnboardingUpsertActor" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "input" },
          },
          type: {
            kind: "NonNullType",
            type: {
              kind: "NamedType",
              name: { kind: "Name", value: "UpsertActorInput" },
            },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "upsertActor" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "input" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "input" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                { kind: "Field", name: { kind: "Name", value: "id" } },
              ],
            },
          },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  OnboardingUpsertActorMutation,
  OnboardingUpsertActorMutationVariables
>;
export const OnboardingUpsertPortDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "mutation",
      name: { kind: "Name", value: "OnboardingUpsertPort" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "input" },
          },
          type: {
            kind: "NonNullType",
            type: {
              kind: "NamedType",
              name: { kind: "Name", value: "UpsertPortInput" },
            },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "upsertPort" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "input" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "input" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                { kind: "Field", name: { kind: "Name", value: "id" } },
              ],
            },
          },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  OnboardingUpsertPortMutation,
  OnboardingUpsertPortMutationVariables
>;
export const OnboardingPortBindingsByPortIdDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "query",
      name: { kind: "Name", value: "OnboardingPortBindingsByPortId" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "portId" },
          },
          type: {
            kind: "NonNullType",
            type: { kind: "NamedType", name: { kind: "Name", value: "Uri" } },
          },
        },
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "first" },
          },
          type: {
            kind: "NonNullType",
            type: { kind: "NamedType", name: { kind: "Name", value: "Int" } },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "portById" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "id" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "portId" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                {
                  kind: "Field",
                  name: { kind: "Name", value: "bindings" },
                  arguments: [
                    {
                      kind: "Argument",
                      name: { kind: "Name", value: "first" },
                      value: {
                        kind: "Variable",
                        name: { kind: "Name", value: "first" },
                      },
                    },
                  ],
                  selectionSet: {
                    kind: "SelectionSet",
                    selections: [
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "edges" },
                        selectionSet: {
                          kind: "SelectionSet",
                          selections: [
                            {
                              kind: "Field",
                              name: { kind: "Name", value: "node" },
                              selectionSet: {
                                kind: "SelectionSet",
                                selections: [
                                  {
                                    kind: "Field",
                                    name: {
                                      kind: "Name",
                                      value: "conversationKey",
                                    },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "actorId" },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "actor" },
                                    selectionSet: {
                                      kind: "SelectionSet",
                                      selections: [
                                        {
                                          kind: "Field",
                                          name: { kind: "Name", value: "id" },
                                        },
                                      ],
                                    },
                                  },
                                ],
                              },
                            },
                          ],
                        },
                      },
                    ],
                  },
                },
              ],
            },
          },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  OnboardingPortBindingsByPortIdQuery,
  OnboardingPortBindingsByPortIdQueryVariables
>;
export const OnboardingActorMessagesDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "query",
      name: { kind: "Name", value: "OnboardingActorMessages" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "actorId" },
          },
          type: {
            kind: "NonNullType",
            type: { kind: "NamedType", name: { kind: "Name", value: "Uri" } },
          },
        },
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "first" },
          },
          type: {
            kind: "NonNullType",
            type: { kind: "NamedType", name: { kind: "Name", value: "Int" } },
          },
        },
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "after" },
          },
          type: { kind: "NamedType", name: { kind: "Name", value: "String" } },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "actor" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "id" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "actorId" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                {
                  kind: "Field",
                  name: { kind: "Name", value: "messages" },
                  arguments: [
                    {
                      kind: "Argument",
                      name: { kind: "Name", value: "first" },
                      value: {
                        kind: "Variable",
                        name: { kind: "Name", value: "first" },
                      },
                    },
                    {
                      kind: "Argument",
                      name: { kind: "Name", value: "after" },
                      value: {
                        kind: "Variable",
                        name: { kind: "Name", value: "after" },
                      },
                    },
                  ],
                  selectionSet: {
                    kind: "SelectionSet",
                    selections: [
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "edges" },
                        selectionSet: {
                          kind: "SelectionSet",
                          selections: [
                            {
                              kind: "Field",
                              name: { kind: "Name", value: "node" },
                              selectionSet: {
                                kind: "SelectionSet",
                                selections: [
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "id" },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "actorId" },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "createdAt" },
                                  },
                                  {
                                    kind: "Field",
                                    name: {
                                      kind: "Name",
                                      value: "messageType",
                                    },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "role" },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "text" },
                                  },
                                  {
                                    kind: "Field",
                                    name: { kind: "Name", value: "payload" },
                                  },
                                ],
                              },
                            },
                          ],
                        },
                      },
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "pageInfo" },
                        selectionSet: {
                          kind: "SelectionSet",
                          selections: [
                            {
                              kind: "Field",
                              name: { kind: "Name", value: "endCursor" },
                            },
                            {
                              kind: "Field",
                              name: { kind: "Name", value: "hasNextPage" },
                            },
                          ],
                        },
                      },
                    ],
                  },
                },
              ],
            },
          },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  OnboardingActorMessagesQuery,
  OnboardingActorMessagesQueryVariables
>;
export const ProvidersListDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "query",
      name: { kind: "Name", value: "ProvidersList" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "first" },
          },
          type: {
            kind: "NonNullType",
            type: { kind: "NamedType", name: { kind: "Name", value: "Int" } },
          },
        },
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "after" },
          },
          type: { kind: "NamedType", name: { kind: "Name", value: "String" } },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "providers" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "first" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "first" },
                },
              },
              {
                kind: "Argument",
                name: { kind: "Name", value: "after" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "after" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                {
                  kind: "Field",
                  name: { kind: "Name", value: "edges" },
                  selectionSet: {
                    kind: "SelectionSet",
                    selections: [
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "node" },
                        selectionSet: {
                          kind: "SelectionSet",
                          selections: [
                            {
                              kind: "FragmentSpread",
                              name: { kind: "Name", value: "ProviderFields" },
                            },
                          ],
                        },
                      },
                    ],
                  },
                },
                {
                  kind: "Field",
                  name: { kind: "Name", value: "pageInfo" },
                  selectionSet: {
                    kind: "SelectionSet",
                    selections: [
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "endCursor" },
                      },
                      {
                        kind: "Field",
                        name: { kind: "Name", value: "hasNextPage" },
                      },
                    ],
                  },
                },
              ],
            },
          },
        ],
      },
    },
    {
      kind: "FragmentDefinition",
      name: { kind: "Name", value: "ProviderFields" },
      typeCondition: {
        kind: "NamedType",
        name: { kind: "Name", value: "Provider" },
      },
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          { kind: "Field", name: { kind: "Name", value: "provider" } },
          { kind: "Field", name: { kind: "Name", value: "providerKind" } },
          { kind: "Field", name: { kind: "Name", value: "apiKey" } },
          { kind: "Field", name: { kind: "Name", value: "baseUrl" } },
          { kind: "Field", name: { kind: "Name", value: "enabled" } },
          { kind: "Field", name: { kind: "Name", value: "tokensUsed" } },
          { kind: "Field", name: { kind: "Name", value: "lastUsed" } },
          { kind: "Field", name: { kind: "Name", value: "defaultTextModel" } },
          { kind: "Field", name: { kind: "Name", value: "defaultAudioModel" } },
          { kind: "Field", name: { kind: "Name", value: "createdAt" } },
          { kind: "Field", name: { kind: "Name", value: "updatedAt" } },
        ],
      },
    },
  ],
} as unknown as DocumentNode<ProvidersListQuery, ProvidersListQueryVariables>;
export const ProviderByKeyDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "query",
      name: { kind: "Name", value: "ProviderByKey" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "provider" },
          },
          type: {
            kind: "NonNullType",
            type: {
              kind: "NamedType",
              name: { kind: "Name", value: "String" },
            },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "provider" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "provider" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "provider" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                {
                  kind: "FragmentSpread",
                  name: { kind: "Name", value: "ProviderFields" },
                },
              ],
            },
          },
        ],
      },
    },
    {
      kind: "FragmentDefinition",
      name: { kind: "Name", value: "ProviderFields" },
      typeCondition: {
        kind: "NamedType",
        name: { kind: "Name", value: "Provider" },
      },
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          { kind: "Field", name: { kind: "Name", value: "provider" } },
          { kind: "Field", name: { kind: "Name", value: "providerKind" } },
          { kind: "Field", name: { kind: "Name", value: "apiKey" } },
          { kind: "Field", name: { kind: "Name", value: "baseUrl" } },
          { kind: "Field", name: { kind: "Name", value: "enabled" } },
          { kind: "Field", name: { kind: "Name", value: "tokensUsed" } },
          { kind: "Field", name: { kind: "Name", value: "lastUsed" } },
          { kind: "Field", name: { kind: "Name", value: "defaultTextModel" } },
          { kind: "Field", name: { kind: "Name", value: "defaultAudioModel" } },
          { kind: "Field", name: { kind: "Name", value: "createdAt" } },
          { kind: "Field", name: { kind: "Name", value: "updatedAt" } },
        ],
      },
    },
  ],
} as unknown as DocumentNode<ProviderByKeyQuery, ProviderByKeyQueryVariables>;
export const UpsertProviderDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "mutation",
      name: { kind: "Name", value: "UpsertProvider" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "input" },
          },
          type: {
            kind: "NonNullType",
            type: {
              kind: "NamedType",
              name: { kind: "Name", value: "UpsertProviderInput" },
            },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "upsertProvider" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "input" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "input" },
                },
              },
            ],
            selectionSet: {
              kind: "SelectionSet",
              selections: [
                {
                  kind: "FragmentSpread",
                  name: { kind: "Name", value: "ProviderFields" },
                },
              ],
            },
          },
        ],
      },
    },
    {
      kind: "FragmentDefinition",
      name: { kind: "Name", value: "ProviderFields" },
      typeCondition: {
        kind: "NamedType",
        name: { kind: "Name", value: "Provider" },
      },
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          { kind: "Field", name: { kind: "Name", value: "provider" } },
          { kind: "Field", name: { kind: "Name", value: "providerKind" } },
          { kind: "Field", name: { kind: "Name", value: "apiKey" } },
          { kind: "Field", name: { kind: "Name", value: "baseUrl" } },
          { kind: "Field", name: { kind: "Name", value: "enabled" } },
          { kind: "Field", name: { kind: "Name", value: "tokensUsed" } },
          { kind: "Field", name: { kind: "Name", value: "lastUsed" } },
          { kind: "Field", name: { kind: "Name", value: "defaultTextModel" } },
          { kind: "Field", name: { kind: "Name", value: "defaultAudioModel" } },
          { kind: "Field", name: { kind: "Name", value: "createdAt" } },
          { kind: "Field", name: { kind: "Name", value: "updatedAt" } },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  UpsertProviderMutation,
  UpsertProviderMutationVariables
>;
export const DeleteProviderDocument = {
  kind: "Document",
  definitions: [
    {
      kind: "OperationDefinition",
      operation: "mutation",
      name: { kind: "Name", value: "DeleteProvider" },
      variableDefinitions: [
        {
          kind: "VariableDefinition",
          variable: {
            kind: "Variable",
            name: { kind: "Name", value: "provider" },
          },
          type: {
            kind: "NonNullType",
            type: {
              kind: "NamedType",
              name: { kind: "Name", value: "String" },
            },
          },
        },
      ],
      selectionSet: {
        kind: "SelectionSet",
        selections: [
          {
            kind: "Field",
            name: { kind: "Name", value: "deleteProvider" },
            arguments: [
              {
                kind: "Argument",
                name: { kind: "Name", value: "provider" },
                value: {
                  kind: "Variable",
                  name: { kind: "Name", value: "provider" },
                },
              },
            ],
          },
        ],
      },
    },
  ],
} as unknown as DocumentNode<
  DeleteProviderMutation,
  DeleteProviderMutationVariables
>;
