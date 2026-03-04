# Borg GraphQL Schema Usage Guide

This guide complements GraphQL introspection descriptions with concrete usage notes and examples.

## Global Conventions

- `Uri` values are strict Borg URIs (`borg:actor:...`, `borg:session:...`, etc).
- List fields use cursor connections:
  - args: `first`, `after`
  - shape: `edges { cursor node }`, `pageInfo { hasNextPage endCursor }`
- Typical pagination flow:
  1. query with `first`
  2. read `pageInfo.endCursor`
  3. call again with `after: endCursor`

## Node Interface

Use `node(id: Uri!)` for cross-entity lookup.

```graphql
query($id: Uri!) {
  node(id: $id) {
    id
    ... on Actor { name status }
    ... on Session { updatedAt }
    ... on App { slug status }
  }
}
```

## Query Recipes

### Actors + Behaviors + Sessions

```graphql
query($actorId: Uri!) {
  actor(id: $actorId) {
    id
    name
    status
    defaultBehavior {
      id
      name
      preferredProvider { provider providerKind }
    }
    sessions(first: 10) {
      edges {
        node {
          id
          updatedAt
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
```

### Ports + Bindings

```graphql
query {
  ports(first: 20) {
    edges {
      node {
        id
        name
        provider
        enabled
        activeSessions
        assignedActor { id name }
        bindings(first: 20) {
          edges {
            node {
              conversationKey
              sessionId
              actor { id name }
            }
          }
        }
      }
    }
  }
}
```

### Providers

```graphql
query {
  providers(first: 20) {
    edges {
      node {
        id
        provider
        providerKind
        enabled
        defaultTextModel
        defaultAudioModel
        tokensUsed
        lastUsed
      }
    }
  }
}
```

### Apps + Capabilities + Connections + Secrets

```graphql
query($slug: String!) {
  appBySlug(slug: $slug) {
    id
    name
    slug
    status
    authStrategy
    availableSecrets
    capabilities(first: 50) {
      edges { node { id name mode status hint } }
    }
    connections(first: 50) {
      edges { node { id status ownerUserId providerAccountId externalUserId } }
    }
    secrets(first: 50) {
      edges { node { id key kind connectionId } }
    }
  }
}
```

### Clockwork Jobs + Runs

```graphql
query {
  clockworkJobs(first: 20, status: "active") {
    edges {
      node {
        id
        kind
        status
        targetActorId
        targetSessionId
        messageType
        nextRunAt
        runs(first: 10) {
          edges { node { id firedAt messageId } }
        }
      }
    }
  }
}
```

### TaskGraph

```graphql
query($taskId: Uri!) {
  task(id: $taskId) {
    id
    title
    description
    status
    labels
    review { submittedAt approvedAt changesRequestedAt }
    parent { id title }
    children(first: 20) { edges { node { id title status } } }
    comments(first: 20) { edges { node { id body authorSessionUri createdAt } } }
    events(first: 20) {
      edges {
        node {
          id
          type
          data {
            kind
            status
            note
            commentId
            subtaskCount
          }
          createdAt
        }
      }
    }
  }
}
```

### Memory Graph + Facts

```graphql
query {
  memoryEntities(queryText: "alice", first: 10) {
    edges {
      node {
        id
        label
        props { key value { kind text integer float boolean reference } }
        facts(first: 20) {
          edges {
            node {
              id
              field
              arity
              value { kind text reference list { kind text } }
              statedAt
            }
          }
        }
      }
    }
  }
}
```

### Policies + Users

```graphql
query {
  policies(first: 20) {
    edges {
      node {
        id
        updatedAt
        uses(first: 20) {
          edges { node { policyId entityId createdAt } }
        }
      }
    }
  }
  users(first: 20) {
    edges { node { id createdAt updatedAt } }
  }
}
```

## Mutation Recipes

### Actors + Behaviors

```graphql
mutation($actorId: Uri!, $behaviorId: Uri!) {
  upsertBehavior(input: {
    id: $behaviorId
    name: "default"
    systemPrompt: "You are concise and practical."
    requiredCapabilities: ["TaskGraph-listTasks"]
    sessionTurnConcurrency: "serial"
    status: "ACTIVE"
  }) { id name status }

  upsertActor(input: {
    id: $actorId
    name: "Planner"
    systemPrompt: "Plan and execute tasks"
    defaultBehaviorId: $behaviorId
    status: "RUNNING"
  }) { id name status }
}
```

### Ports + Bindings

```graphql
mutation($conversation: Uri!, $session: Uri!, $actor: Uri!) {
  upsertPort(input: {
    name: "telegram"
    provider: "telegram"
    enabled: true
    allowsGuests: false
    assignedActorId: $actor
    settings: { allowed_external_user_ids: ["@leostera"] }
  }) { id name enabled }

  upsertPortBinding(input: {
    portName: "telegram"
    conversationKey: $conversation
    sessionId: $session
  }) { conversationKey sessionId }

  upsertPortActorBinding(input: {
    portName: "telegram"
    conversationKey: $conversation
    actorId: $actor
  }) { conversationKey actorId }
}
```

### Providers

```graphql
mutation {
  upsertProvider(input: {
    provider: "openai"
    providerKind: "openai"
    apiKey: "sk-***"
    baseUrl: "https://api.openai.com/v1"
    enabled: true
    defaultTextModel: "gpt-4.1-mini"
    defaultAudioModel: "gpt-4o-mini-transcribe"
  }) {
    provider
    enabled
    defaultTextModel
  }
}
```

### Apps

```graphql
mutation($appId: Uri!, $capId: Uri!, $connId: Uri!, $secretId: Uri!, $owner: Uri) {
  upsertApp(input: {
    id: $appId
    name: "GitHub"
    slug: "github"
    description: "GitHub integration"
    status: "ACTIVE"
    builtIn: false
    source: "custom"
    authStrategy: "oauth2"
    authConfig: { auth_url: "https://github.com/login/oauth/authorize" }
    availableSecrets: ["GITHUB_TOKEN"]
  }) { id slug }

  upsertAppCapability(input: {
    appId: $appId
    capabilityId: $capId
    name: "issues.list"
    hint: "List issues"
    mode: "READ"
    instructions: "Prefer filtered issue reads"
    status: "ACTIVE"
  }) { id name }

  upsertAppConnection(input: {
    appId: $appId
    connectionId: $connId
    ownerUserId: $owner
    providerAccountId: "acct_123"
    externalUserId: "octocat"
    status: "CONNECTED"
    connection: { oauth_state: "state-123" }
  }) { id status }

  upsertAppSecret(input: {
    appId: $appId
    secretId: $secretId
    connectionId: $connId
    key: "GITHUB_TOKEN"
    value: "ghp_***"
    kind: "token"
  }) { id key kind }
}
```

### Sessions + Messages

```graphql
mutation($session: Uri!, $user: Uri!, $port: Uri!) {
  upsertSession(input: {
    sessionId: $session
    users: [$user]
    port: $port
  }) { id users portId }

  appendSessionMessage(input: {
    sessionId: $session
    messageType: "user"
    role: "user"
    text: "Hello Borg"
  }) {
    id
    messageIndex
    messageType
    role
    text
  }
}
```

### Clockwork

```graphql
mutation($actor: Uri!, $session: Uri!) {
  createClockworkJob(input: {
    jobId: "daily-digest"
    kind: "cron"
    actorId: $actor
    sessionId: $session
    messageType: "user"
    payload: { text: "Send digest" }
    headers: { source: "graphql" }
    scheduleSpec: { cron: "0 9 * * *" }
  }) { id status nextRunAt }

  pauseClockworkJob(jobId: "daily-digest")
  resumeClockworkJob(jobId: "daily-digest")
}
```

### TaskGraph

```graphql
mutation($session: Uri!, $creator: Uri!, $assignee: Uri!) {
  createTask(input: {
    sessionUri: $session
    creatorAgentId: $creator
    assigneeAgentId: $assignee
    title: "Ship schema docs"
    description: "Document every operation"
    definitionOfDone: "Docs + examples merged"
    labels: ["area:api", "priority:high"]
  }) { id title status assigneeSessionId }
}
```

Then transition status:

```graphql
mutation($task: Uri!, $session: Uri!) {
  setTaskStatus(input: {
    taskId: $task
    sessionUri: $session
    status: DOING
  }) { id status }
}
```

## Type-by-Type Reference

This section is a quick schema map so frontend/SDK engineers can find every entity, relation, and its common usage shape.

### Node

Usage notes:
- `Node` is the shared interface for URI-addressable entities.
- Use inline fragments to access concrete fields.

```graphql
query($id: Uri!) {
  node(id: $id) {
    id
    ... on Actor { name status }
    ... on Session { updatedAt }
    ... on Task { title status }
  }
}
```

### Actor and Behavior

Usage notes:
- `Actor.defaultBehavior` resolves to `Behavior`.
- `Actor.sessions` gives actor-participation history.
- `Behavior.preferredProvider` resolves provider metadata.

```graphql
query($actor: Uri!, $behavior: Uri!) {
  actor(id: $actor) {
    id
    name
    systemPrompt
    status
    defaultBehavior { id name preferredProviderId }
    sessions(first: 10) { edges { node { id updatedAt } } }
  }
  behavior(id: $behavior) {
    id
    name
    systemPrompt
    requiredCapabilities
    preferredProvider { provider providerKind enabled }
  }
}
```

### Session and SessionMessage

Usage notes:
- `Session.messages` is the canonical chat/event timeline.
- Prefer typed message fields (`messageType`, `role`, `text`) over `payload`.

```graphql
query($session: Uri!) {
  session(id: $session) {
    id
    users
    portId
    port { id name provider }
    messages(first: 25) {
      edges {
        node {
          id
          messageIndex
          createdAt
          messageType
          role
          text
        }
      }
      pageInfo { hasNextPage endCursor }
    }
  }
}
```

### Port, PortBinding, PortActorBinding

Usage notes:
- `Port.bindings` maps `conversationKey -> sessionId`.
- `Port.actorBindings` maps `conversationKey -> actorId`.
- `PortBinding.actor` resolves through actor-binding table for convenience.

```graphql
query($port: String!) {
  port(name: $port) {
    id
    provider
    enabled
    allowsGuests
    assignedActor { id name }
    bindings(first: 20) {
      edges {
        node {
          id
          conversationKey
          sessionId
          session { id updatedAt }
          actor { id name }
        }
      }
    }
    actorBindings(first: 20) {
      edges {
        node {
          id
          conversationKey
          actorId
          actor { id name status }
        }
      }
    }
  }
}
```

### Provider

Usage notes:
- `provider` is the stable config key (`openai`, `openrouter`, ...).
- `providerKind` captures adapter family when different from key.

```graphql
query {
  providers(first: 20) {
    edges {
      node {
        id
        provider
        providerKind
        baseUrl
        enabled
        tokensUsed
        lastUsed
        defaultTextModel
        defaultAudioModel
      }
    }
  }
}
```

### App, AppCapability, AppConnection, AppSecret

Usage notes:
- `App` is the parent entity.
- Capability, connection, and secret data are reachable from `App`.
- `authConfig` / `connection` are transitional JSON fields.

```graphql
query($slug: String!) {
  appBySlug(slug: $slug) {
    id
    name
    slug
    description
    status
    source
    authStrategy
    availableSecrets
    capabilities(first: 20) {
      edges { node { id appId name hint mode instructions status } }
    }
    connections(first: 20) {
      edges { node { id appId ownerUserId providerAccountId externalUserId status } }
    }
    secrets(first: 20) {
      edges { node { id appId connectionId key kind createdAt } }
    }
  }
}
```

### ClockworkJob and ClockworkJobRun

Usage notes:
- `ClockworkJob` describes schedule + target.
- `ClockworkJob.runs` gives execution history.

```graphql
query($jobId: String!) {
  clockworkJob(jobId: $jobId) {
    id
    kind
    status
    targetActorId
    targetSessionId
    messageType
    nextRunAt
    lastRunAt
    runs(first: 10) {
      edges {
        node {
          id
          jobId
          scheduledFor
          firedAt
          targetActorId
          targetSessionId
          messageId
        }
      }
    }
  }
}
```

### Task, TaskComment, TaskEvent, TaskEventDataObject

Usage notes:
- `Task` is the central taskgraph node.
- Use `parent` and `children` for DAG traversal.
- `TaskEvent.data.kind` determines which optional event payload fields are populated.

```graphql
query($task: Uri!) {
  task(id: $task) {
    id
    title
    description
    definitionOfDone
    status
    assigneeAgentId
    assigneeSessionId
    reviewerAgentId
    reviewerSessionId
    labels
    parentUri
    blockedBy
    duplicateOf
    references
    review { submittedAt approvedAt changesRequestedAt }
    parent { id title }
    children(first: 20) { edges { node { id title status } } }
    comments(first: 20) {
      edges { node { id taskUri authorSessionUri body createdAt } }
    }
    events(first: 20) {
      edges {
        node {
          id
          taskUri
          actorSessionUri
          type
          data {
            kind
            assigneeAgentId
            status
            note
            commentId
            subtaskCount
          }
          createdAt
        }
      }
    }
  }
}
```

### MemoryEntity, MemoryFact, MemoryValueObject

Usage notes:
- Entity properties are exposed as typed key/value rows in `props`.
- Fact values are normalized to `MemoryValueObject` with `kind` discriminator.
- For list values, recurse through `list`.

```graphql
query($entity: Uri!) {
  memoryEntity(id: $entity) {
    id
    entityType
    label
    props {
      key
      value {
        kind
        text
        integer
        float
        boolean
        bytesBase64
        reference
        date
        datetime
        list { kind text reference }
      }
    }
    facts(first: 20, includeRetracted: false) {
      edges {
        node {
          id
          source
          entity
          field
          arity
          value { kind text reference list { kind text } }
          txId
          statedAt
          isRetracted
        }
      }
    }
  }
}
```

### Policy, PolicyUse, User

Usage notes:
- `Policy.uses` links policies to target entities.
- `User.profile` is currently transitional JSON.

```graphql
query {
  policies(first: 20) {
    edges {
      node {
        id
        createdAt
        updatedAt
        uses(first: 20) {
          edges { node { policyId entityId createdAt } }
        }
      }
    }
  }
  users(first: 20) {
    edges {
      node {
        id
        createdAt
        updatedAt
      }
    }
  }
}
```

## Input Reference (Mutation Arguments)

Usage notes:
- All URI-like IDs are `Uri` scalar validated at parse-time.
- Prefer typed fields in message/task payloads over free-form JSON.
- Inputs below map 1:1 to mutation fields in `MutationRoot`.

### Control plane

`UpsertActorInput`, `UpsertBehaviorInput`, `UpsertPortInput`, `UpsertPortBindingInput`, `UpsertPortActorBindingInput`, `UpsertProviderInput`, `UpsertAppInput`, `UpsertAppCapabilityInput`, `UpsertAppConnectionInput`, `UpsertAppSecretInput`, `UpsertSessionInput`.

```graphql
mutation($actor: Uri!, $behavior: Uri!, $app: Uri!, $cap: Uri!, $session: Uri!, $user: Uri!, $port: Uri!) {
  upsertBehavior(input: {
    id: $behavior
    name: "default"
    systemPrompt: "You are concise."
    preferredProviderId: "openai"
    requiredCapabilities: ["TaskGraph-listTasks"]
    sessionTurnConcurrency: "serial"
    status: "ACTIVE"
  }) { id }

  upsertActor(input: {
    id: $actor
    name: "Planner"
    systemPrompt: "Plan and execute."
    defaultBehaviorId: $behavior
    status: "RUNNING"
  }) { id }

  upsertSession(input: {
    sessionId: $session
    users: [$user]
    port: $port
  }) { id }

  upsertAppCapability(input: {
    appId: $app
    capabilityId: $cap
    name: "issues.list"
    hint: "List issues"
    mode: "READ"
    instructions: "Prefer filtered issue reads."
    status: "ACTIVE"
  }) { id }
}
```

### Messaging + Clockwork + Taskgraph

`SessionMessageInput`, `AppendSessionMessageInput`, `PatchSessionMessageInput`, `CreateClockworkJobInputGql`, `UpdateClockworkJobInputGql`, `CreateTaskInputGql`, `UpdateTaskInputGql`, `SetTaskStatusInput`.

```graphql
mutation($session: Uri!, $creator: Uri!, $assignee: Uri!, $task: Uri!, $actor: Uri!) {
  appendSessionMessage(input: {
    sessionId: $session
    messageType: "user"
    role: "user"
    text: "Please summarize the backlog."
  }) { id messageIndex }

  createClockworkJob(input: {
    jobId: "daily-summary"
    kind: "cron"
    actorId: $actor
    sessionId: $session
    messageType: "user"
    payload: { text: "Daily summary" }
    scheduleSpec: { cron: "0 9 * * 1-5" }
  }) { id status }

  createTask(input: {
    sessionUri: $session
    creatorAgentId: $creator
    assigneeAgentId: $assignee
    title: "Ship schema docs"
    description: "Document every entity and mutation."
    definitionOfDone: "Docs merged and tests green."
    labels: ["area:api", "type:docs"]
  }) { id status }

  updateTask(input: {
    taskId: $task
    sessionUri: $session
    title: "Ship complete schema docs"
  }) { id title }
}
```

### Runtime wrapper placeholders

`RunActorChatInput` and `RunPortHttpInput` are present for forward-compatibility.

Current behavior:
- They return GraphQL error `BAD_REQUEST` in standalone `borg-gql`.
- Keep generated clients using these shapes so later runtime integration is non-breaking.

```graphql
mutation($actor: Uri!, $session: Uri!, $user: Uri!) {
  runActorChat(input: {
    actorId: $actor
    sessionId: $session
    userId: $user
    text: "Hello"
  }) { ok message }
}
```

## Subscription Recipes (Real-time)

Subscriptions are exposed from `SubscriptionRoot` and intended for WebSocket transport.

### Session chat stream

Use `sessionChat` to stream new timeline rows as they are appended.

Usage notes:
- Omit `afterMessageIndex` to start tail-follow mode from "now".
- Provide `afterMessageIndex` to resume from a known checkpoint.
- `pollIntervalMs` is clamped server-side for safety.

```graphql
subscription($session: Uri!, $after: Int) {
  sessionChat(sessionId: $session, afterMessageIndex: $after, pollIntervalMs: 500) {
    id
    messageIndex
    messageType
    role
    text
    createdAt
  }
}
```

### Session notifications stream

Use `sessionNotifications` for a notification-friendly stream derived from session messages.

Usage notes:
- By default, user-authored messages are filtered out.
- Set `includeUserMessages: true` to receive all message roles.
- `kind` gives stable UI routing (`ASSISTANT_REPLY`, `TOOL_ACTIVITY`, `SESSION_EVENT`, `MESSAGE`).

```graphql
subscription($session: Uri!) {
  sessionNotifications(sessionId: $session, pollIntervalMs: 500) {
    id
    kind
    title
    messageType
    role
    text
    createdAt
    sessionMessage { messageIndex messageType role }
  }
}
```

## Transitional JSON Fields

Some fields are still `JsonValue` for compatibility with legacy DB columns (for example app auth config, session message payload, policy JSON).

Recommendations:
- Prefer typed fields when available (`messageType`, `role`, `text`, typed relation fields).
- Treat `JsonValue` fields as transitional and avoid new feature coupling to them.

## Runtime Wrapper Mutations

`runActorChat` and `runPortHttp` are currently placeholders in standalone `borg-gql`.

Current behavior:
- return structured GraphQL error with `code = BAD_REQUEST`.

Planned behavior:
- map to existing runtime execution flows once integrated into `borg-api`.
