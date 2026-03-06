import {
  ActorStatusValue,
  requestGraphQL,
  resolveDefaultBaseUrl,
} from "@borg/graphql-client";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  JsonTreeViewer,
  Label,
  ScrollArea,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
} from "@borg/ui";
import React from "react";
import ReactFlow, {
  Background,
  Controls,
  MiniMap,
  type Edge,
  type Node,
  type NodeProps,
  type NodeTypes,
} from "reactflow";

const STAGE_USER_KEY = "borg:user:stage";
const STAGE_QUERY_ACTORS = `
  query StageActors($first: Int!) {
    actors(first: $first) {
      edges {
        node {
          id
          name
          status
          createdAt
          updatedAt
          sessions(first: 1) {
            edges {
              node {
                id
                updatedAt
              }
            }
          }
        }
      }
    }
  }
`;

const STAGE_QUERY_ACTOR_MAILBOX = `
  query StageActorMailbox($actorId: Uri!, $messageFirst: Int!) {
    actor(id: $actorId) {
      id
      name
      status
      sessions(first: 1) {
        edges {
          node {
            id
            updatedAt
            messages(first: $messageFirst) {
              edges {
                node {
                  id
                  createdAt
                  messageType
                  role
                  text
                  payload
                }
              }
            }
          }
        }
      }
    }
  }
`;

const STAGE_MUTATION_UPSERT_ACTOR = `
  mutation StageUpsertActor($input: UpsertActorInput!) {
    upsertActor(input: $input) {
      id
      name
      status
    }
  }
`;

type RuntimeStatus = "checking" | "online" | "offline";

type ActorSummary = {
  id: string;
  name: string;
  status: string;
  createdAt: string;
  updatedAt: string;
  sessionId: string | null;
};

type MailboxMessage = {
  id: string;
  createdAt: string;
  messageType: string;
  role: string | null;
  text: string | null;
  payload: unknown;
};

type ActorSession = {
  id: string;
  updatedAt: string;
  messages: MailboxMessage[];
};

type ActorMailbox = {
  actorId: string;
  actorName: string;
  actorStatus: string;
  session: ActorSession | null;
};

type CreateActorDraft = {
  actorId: string;
  name: string;
  provider: string;
  model: string;
  status: ActorStatusValue;
  systemPrompt: string;
};

type StageActorsResponse = {
  actors: {
    edges: Array<{
      node: {
        id: string;
        name: string;
        status: string;
        createdAt: string;
        updatedAt: string;
        sessions: {
          edges: Array<{
            node: {
              id: string;
              updatedAt: string;
            };
          }>;
        };
      };
    }>;
  };
};

type StageActorMailboxResponse = {
  actor: {
    id: string;
    name: string;
    status: string;
    sessions: {
      edges: Array<{
        node: {
          id: string;
          updatedAt: string;
          messages: {
            edges: Array<{
              node: {
                id: string;
                createdAt: string;
                messageType: string;
                role: string | null;
                text: string | null;
                payload: unknown;
              };
            }>;
          };
        };
      }>;
    };
  } | null;
};

type StageUpsertActorResponse = {
  upsertActor: {
    id: string;
    name: string;
    status: string;
  };
};

type ActorNodeData = {
  actor: ActorSummary;
};

type ActorTab = "details" | "mailbox";

type ToolField = {
  key: string;
  value: string;
};

type MailboxEntry =
  | {
      kind: "message";
      key: string;
      message: MailboxMessage;
    }
  | {
      kind: "tool";
      key: string;
      role: string | null;
      createdAt: string;
      toolName: string;
      fields: ToolField[];
      sourceType: "tool_call" | "tool_result";
    };

const DEFAULT_CREATE_ACTOR_DRAFT: CreateActorDraft = {
  actorId: "",
  name: "",
  provider: "openai",
  model: "gpt-5",
  status: ActorStatusValue.Running,
  systemPrompt: "You are a pragmatic actor. Be precise and action-oriented.",
};

const CREATE_ACTOR_STATUS_OPTIONS: Array<{
  value: ActorStatusValue;
  label: string;
}> = [
  { value: ActorStatusValue.Running, label: "RUNNING" },
  { value: ActorStatusValue.Paused, label: "PAUSED" },
  { value: ActorStatusValue.Disabled, label: "DISABLED" },
  { value: ActorStatusValue.Error, label: "ERROR" },
];

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function formatTimeSince(value: string | null): string {
  if (!value) {
    return "Unknown";
  }

  const startedAt = new Date(value);
  if (Number.isNaN(startedAt.getTime())) {
    return "Unknown";
  }

  const elapsedMs = Date.now() - startedAt.getTime();
  if (elapsedMs < 0) {
    return "Just now";
  }

  const minutes = Math.floor(elapsedMs / 60000);
  if (minutes < 1) {
    return "< 1 min";
  }
  if (minutes < 60) {
    return `${minutes} min`;
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} hr`;
  }

  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${days} day${days === 1 ? "" : "s"}`;
  }

  const months = Math.floor(days / 30);
  return `${months} month${months === 1 ? "" : "s"}`;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Unexpected error";
}

function slugifyActorSegment(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function normalizeActorId(rawActorId: string, fallbackName: string): string {
  const explicit = rawActorId.trim();
  if (explicit.length > 0) {
    if (explicit.includes(":")) {
      return explicit;
    }
    const slug = slugifyActorSegment(explicit);
    return slug.length > 0 ? `borg:actor:${slug}` : "";
  }

  const slug = slugifyActorSegment(fallbackName);
  return slug.length > 0 ? `borg:actor:${slug}` : "";
}

function statusBadgeClass(status: string): string {
  const normalized = status.toUpperCase();
  if (normalized === "RUNNING") {
    return "bg-emerald-500/15 text-emerald-700";
  }
  if (normalized === "PAUSED") {
    return "bg-amber-500/15 text-amber-700";
  }
  if (normalized === "ERROR") {
    return "bg-rose-500/15 text-rose-700";
  }
  if (normalized === "DISABLED") {
    return "bg-zinc-500/15 text-zinc-700";
  }
  return "bg-zinc-500/10 text-zinc-700";
}

function parseJsonPayload(payload: unknown): unknown {
  if (typeof payload !== "string") {
    return payload;
  }

  const trimmed = payload.trim();
  if (!trimmed) {
    return payload;
  }

  try {
    return JSON.parse(trimmed);
  } catch {
    return payload;
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function pickString(
  object: Record<string, unknown> | null,
  keys: string[]
): string | null {
  if (!object) {
    return null;
  }
  for (const key of keys) {
    const value = object[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return value.trim();
    }
  }
  return null;
}

function extractToolName(payload: unknown): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  return pickString(object, ["name", "tool_name", "toolName"]);
}

function extractPayloadType(payload: unknown): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  return pickString(object, [
    "type",
    "messageType",
    "message_type",
    "kind",
    "event",
  ]);
}

function extractToolArguments(payload: unknown): unknown {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  if (!object) {
    return parsed;
  }
  const argumentsRaw =
    object.arguments ?? object.args ?? object.input ?? object.params;
  return parseJsonPayload(argumentsRaw ?? object);
}

function extractToolResult(payload: unknown): unknown {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  if (!object) {
    return parsed;
  }
  const contentRaw =
    object.content ??
    object.output ??
    object.result ??
    object.payload ??
    object.text ??
    object.Text;
  return parseJsonPayload(contentRaw ?? object);
}

function extractTargetActorIdFromPayload(payload: unknown): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  if (!object) {
    return null;
  }

  const nested = parseJsonPayload(
    object.arguments ?? object.args ?? object.input ?? null
  );
  const nestedObject = asRecord(nested);
  const fromNested = pickString(nestedObject, [
    "targetActorId",
    "target_actor_id",
    "actorId",
    "actor_id",
    "receiverId",
    "receiver_id",
    "to",
  ]);
  if (fromNested) {
    return fromNested;
  }

  return pickString(object, [
    "targetActorId",
    "target_actor_id",
    "actorId",
    "actor_id",
  ]);
}

function normalizeToolGroup(toolName: string): string {
  const trimmed = toolName.trim();
  if (!trimmed) {
    return "Tool";
  }
  const dash = trimmed.indexOf("-");
  if (dash > 0) {
    return trimmed.slice(0, dash);
  }
  const colon = trimmed.indexOf(":");
  if (colon > 0) {
    return trimmed.slice(0, colon);
  }
  return trimmed;
}

function isSessionEventMessage(message: MailboxMessage): boolean {
  return message.messageType.toLowerCase() === "session_event";
}

function isToolCallMessage(message: MailboxMessage): boolean {
  const messageType = message.messageType.toLowerCase();
  if (messageType.includes("tool_call")) {
    return true;
  }

  const payloadType = (extractPayloadType(message.payload) ?? "").toLowerCase();
  if (payloadType === "tool_call") {
    return true;
  }
  if (payloadType === "tool_result") {
    return false;
  }

  return extractToolName(message.payload) !== null;
}

function isToolResultMessage(message: MailboxMessage): boolean {
  const messageType = message.messageType.toLowerCase();
  if (messageType.includes("tool_result")) {
    return true;
  }

  const payloadType = (extractPayloadType(message.payload) ?? "").toLowerCase();
  return payloadType === "tool_result";
}

function formatMessageFieldKey(rawKey: string): string {
  return rawKey
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[-\s]+/g, "_")
    .toLowerCase();
}

function formatPrimitiveValue(value: unknown): string | null {
  if (typeof value === "string") {
    return value;
  }
  if (
    typeof value === "number" ||
    typeof value === "boolean" ||
    typeof value === "bigint"
  ) {
    return String(value);
  }
  if (value === null) {
    return "null";
  }
  if (value === undefined) {
    return "undefined";
  }
  return null;
}

function safeStringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function pickUnknown(
  object: Record<string, unknown> | null,
  keys: string[]
): unknown {
  if (!object) {
    return undefined;
  }
  for (const key of keys) {
    if (key in object && object[key] !== undefined) {
      return object[key];
    }
  }
  return undefined;
}

function unwrapToolEnvelope(value: unknown): unknown {
  let current = parseJsonPayload(value);

  for (let depth = 0; depth < 3; depth += 1) {
    const object = asRecord(current);
    if (!object) {
      return current;
    }

    const nested = pickUnknown(object, [
      "content",
      "output",
      "result",
      "payload",
      "text",
      "Text",
      "data",
    ]);

    if (nested === undefined) {
      return current;
    }

    const parsedNested = parseJsonPayload(nested);
    if (parsedNested === current) {
      return current;
    }
    if (parsedNested === nested) {
      return nested;
    }

    current = parsedNested;
  }

  return current;
}

function formatToolDisplayName(toolName: string): string {
  const trimmed = toolName.trim();
  const dashIndex = trimmed.indexOf("-");
  if (dashIndex <= 0) {
    return trimmed;
  }
  return `${trimmed.slice(0, dashIndex)}:${trimmed.slice(dashIndex + 1)}`;
}

const TOOL_ENVELOPE_KEYS = new Set([
  "type",
  "event",
  "kind",
  "name",
  "tool_name",
  "toolName",
  "messageType",
  "message_type",
  "content",
  "text",
  "Text",
  "output",
  "result",
  "payload",
  "data",
]);

function toToolFields(value: unknown): ToolField[] {
  const unwrapped = unwrapToolEnvelope(value);
  const object = asRecord(unwrapped);

  if (object) {
    const fields = Object.entries(object)
      .filter(([key]) => !TOOL_ENVELOPE_KEYS.has(key))
      .map(([key, rawValue]) => ({
        key: formatMessageFieldKey(key),
        value: formatPrimitiveValue(rawValue) ?? safeStringify(rawValue),
      }))
      .filter((field) => field.value.trim().length > 0);

    if (fields.length > 0) {
      return fields;
    }
  }

  const primitive = formatPrimitiveValue(unwrapped);
  if (primitive !== null && primitive.trim().length > 0) {
    return [{ key: "result", value: primitive }];
  }

  if (unwrapped === null || unwrapped === undefined) {
    return [];
  }

  return [{ key: "result", value: safeStringify(unwrapped) }];
}

function ActorNodeCard({ data, selected }: NodeProps<ActorNodeData>) {
  const actor = data.actor;

  return (
    <div
      className={`stage-card w-56 rounded-2xl border px-3 py-2 shadow-md transition ${
        selected
          ? "border-sky-400 bg-sky-50"
          : "border-slate-200 bg-white/90 hover:border-slate-300"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="truncate text-xs font-semibold">{actor.name}</p>
        <Badge className={statusBadgeClass(actor.status)}>{actor.status}</Badge>
      </div>
      <p className="mt-1 truncate text-[11px] text-slate-500">{actor.id}</p>
      <p className="mt-1 text-[11px] text-slate-500">
        session {actor.sessionId ? "ready" : "pending"} · updated{" "}
        {formatDate(actor.updatedAt)}
      </p>
    </div>
  );
}

const actorNodeTypes: NodeTypes = {
  actor: ActorNodeCard,
};

export function App() {
  const [actors, setActors] = React.useState<ActorSummary[]>([]);
  const [runtimeStatus, setRuntimeStatus] =
    React.useState<RuntimeStatus>("checking");
  const [runtimeMessage, setRuntimeMessage] = React.useState(
    "Checking Borg runtime..."
  );
  const [isLoadingActors, setIsLoadingActors] = React.useState(true);
  const [isLoadingMailbox, setIsLoadingMailbox] = React.useState(false);
  const [isSending, setIsSending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const [selectedActorId, setSelectedActorId] = React.useState<string | null>(
    null
  );
  const [mailbox, setMailbox] = React.useState<ActorMailbox | null>(null);
  const [actorGraphMessages, setActorGraphMessages] = React.useState<
    Record<string, MailboxMessage[]>
  >({});
  const [optimisticMessages, setOptimisticMessages] = React.useState<
    Record<string, MailboxMessage[]>
  >({});
  const [draft, setDraft] = React.useState("");
  const [activeTab, setActiveTab] = React.useState<ActorTab>("mailbox");

  const [isCreateActorOpen, setIsCreateActorOpen] = React.useState(false);
  const [isCreatingActor, setIsCreatingActor] = React.useState(false);
  const [createActorDraft, setCreateActorDraft] = React.useState(
    DEFAULT_CREATE_ACTOR_DRAFT
  );

  const baseUrl = React.useMemo(() => resolveDefaultBaseUrl(), []);
  const mailboxEndRef = React.useRef<HTMLDivElement | null>(null);

  const createActorIdPreview = React.useMemo(
    () => normalizeActorId(createActorDraft.actorId, createActorDraft.name),
    [createActorDraft.actorId, createActorDraft.name]
  );

  const scrollMailboxToBottom = React.useCallback((smooth = true) => {
    mailboxEndRef.current?.scrollIntoView({
      behavior: smooth ? "smooth" : "auto",
      block: "end",
    });
  }, []);

  const loadActorGraphMessages = React.useCallback(async (actorIds: string[]) => {
    if (actorIds.length === 0) {
      setActorGraphMessages({});
      return;
    }

    const entries: Array<[string, MailboxMessage[]]> = await Promise.all(
      actorIds.map(async (actorId) => {
        try {
          const data = await requestGraphQL<
            StageActorMailboxResponse,
            { actorId: string; messageFirst: number }
          >({
            query: STAGE_QUERY_ACTOR_MAILBOX,
            variables: {
              actorId,
              messageFirst: 160,
            },
          });

          const messages =
            data.actor?.sessions.edges[0]?.node.messages.edges
              .map((edge) => edge.node)
              .map((message) => ({
                id: message.id,
                createdAt: message.createdAt,
                messageType: message.messageType,
                role: message.role,
                text: message.text,
                payload: message.payload,
              }))
              .filter((message) => !isSessionEventMessage(message)) ?? [];
          return [actorId, messages];
        } catch {
          return [actorId, []];
        }
      })
    );

    const next: Record<string, MailboxMessage[]> = {};
    for (const [actorId, messages] of entries) {
      next[actorId] = messages;
    }
    setActorGraphMessages(next);
  }, []);

  const loadActors = React.useCallback(async () => {
    setIsLoadingActors(true);
    setError(null);

    try {
      const data = await requestGraphQL<StageActorsResponse, { first: number }>(
        {
          query: STAGE_QUERY_ACTORS,
          variables: { first: 200 },
        }
      );

      const nextActors: ActorSummary[] = data.actors.edges
        .map((edge) => edge.node)
        .map((node) => ({
          id: node.id,
          name: node.name,
          status: node.status,
          createdAt: node.createdAt,
          updatedAt: node.updatedAt,
          sessionId: node.sessions.edges[0]?.node.id ?? null,
        }));

      setActors(nextActors);
      void loadActorGraphMessages(nextActors.map((actor) => actor.id));
      setRuntimeStatus("online");
      setRuntimeMessage(`Connected to ${baseUrl || "same-origin"}`);

      setSelectedActorId((previous) => {
        if (!previous) {
          return nextActors[0]?.id ?? null;
        }
        return nextActors.some((actor) => actor.id === previous)
          ? previous
          : (nextActors[0]?.id ?? null);
      });
    } catch (loadError) {
      setRuntimeStatus("offline");
      setRuntimeMessage(errorMessage(loadError));
      setError(errorMessage(loadError));
    } finally {
      setIsLoadingActors(false);
    }
  }, [baseUrl, loadActorGraphMessages]);

  const loadMailbox = React.useCallback(async (actorId: string) => {
    setIsLoadingMailbox(true);
    setError(null);

    try {
      const data = await requestGraphQL<
        StageActorMailboxResponse,
        { actorId: string; messageFirst: number }
      >({
        query: STAGE_QUERY_ACTOR_MAILBOX,
        variables: {
          actorId,
          messageFirst: 300,
        },
      });

      if (!data.actor) {
        setMailbox(null);
        return;
      }

      const sessionNode = data.actor.sessions.edges[0]?.node;
      const session: ActorSession | null = sessionNode
        ? {
            id: sessionNode.id,
            updatedAt: sessionNode.updatedAt,
            messages: sessionNode.messages.edges
              .map((messageEdge) => messageEdge.node)
              .map((message) => ({
                id: message.id,
                createdAt: message.createdAt,
                messageType: message.messageType,
                role: message.role,
                text: message.text,
                payload: message.payload,
              }))
              .filter((message) => !isSessionEventMessage(message))
              .sort((left, right) =>
                left.createdAt.localeCompare(right.createdAt)
              ),
          }
        : null;

      setMailbox({
        actorId: data.actor.id,
        actorName: data.actor.name,
        actorStatus: data.actor.status,
        session,
      });
      setOptimisticMessages((current) => {
        if (!current[actorId]) {
          return current;
        }
        const next = { ...current };
        delete next[actorId];
        return next;
      });
    } catch (mailboxError) {
      setError(errorMessage(mailboxError));
    } finally {
      setIsLoadingMailbox(false);
    }
  }, []);

  const createActor = React.useCallback(async () => {
    const actorId = normalizeActorId(createActorDraft.actorId, createActorDraft.name);
    const actorName = createActorDraft.name.trim() || actorId;
    const provider = createActorDraft.provider.trim();
    const model = createActorDraft.model.trim();
    const systemPrompt = createActorDraft.systemPrompt.trim();

    if (!actorId) {
      setError("Provide an actor name or actor id.");
      return;
    }

    if (!systemPrompt) {
      setError("System prompt is required to create an actor.");
      return;
    }
    if (!provider || !model) {
      setError("Provider and model are required to create an actor.");
      return;
    }

    const systemPromptWithRuntimeHints = [
      `[runtime.provider] ${provider}`,
      `[runtime.model] ${model}`,
      "",
      systemPrompt,
    ].join("\n");

    setIsCreatingActor(true);
    setError(null);

    try {
      await requestGraphQL<
        StageUpsertActorResponse,
        {
          input: {
            id: string;
            name: string;
            status: ActorStatusValue;
            systemPrompt: string;
          };
        }
      >({
        query: STAGE_MUTATION_UPSERT_ACTOR,
        variables: {
          input: {
            id: actorId,
            name: actorName,
            status: createActorDraft.status,
            systemPrompt: systemPromptWithRuntimeHints,
          },
        },
      });

      setIsCreateActorOpen(false);
      setCreateActorDraft(DEFAULT_CREATE_ACTOR_DRAFT);
      setSelectedActorId(actorId);
      setActiveTab("mailbox");

      await Promise.all([loadActors(), loadMailbox(actorId)]);
    } catch (createError) {
      setError(errorMessage(createError));
    } finally {
      setIsCreatingActor(false);
    }
  }, [createActorDraft, loadActors, loadMailbox]);

  React.useEffect(() => {
    void loadActors();

    const timer = setInterval(() => {
      void loadActors();
    }, 12000);

    return () => clearInterval(timer);
  }, [loadActors]);

  React.useEffect(() => {
    if (!selectedActorId) {
      setMailbox(null);
      return;
    }

    void loadMailbox(selectedActorId);

    const timer = setInterval(() => {
      void loadMailbox(selectedActorId);
    }, 8000);

    return () => clearInterval(timer);
  }, [loadMailbox, selectedActorId]);

  React.useEffect(() => {
    setActiveTab("mailbox");
  }, [selectedActorId]);

  const sendMessage = React.useCallback(async () => {
    const text = draft.trim();
    if (!text || !selectedActorId) {
      return;
    }

    const actorId = selectedActorId;
    const optimisticMessageId = `optimistic:${Date.now()}:${Math.floor(
      Math.random() * 1000
    )}`;
    const optimisticMessage: MailboxMessage = {
      id: optimisticMessageId,
      createdAt: new Date().toISOString(),
      messageType: "user",
      role: "user",
      text,
      payload: {
        type: "user",
        content: text,
      },
    };

    setDraft("");
    setActiveTab("mailbox");
    setOptimisticMessages((current) => {
      const previous = current[actorId] ?? [];
      return {
        ...current,
        [actorId]: [...previous, optimisticMessage],
      };
    });
    requestAnimationFrame(() => {
      scrollMailboxToBottom(false);
    });

    setIsSending(true);
    setError(null);

    try {
      const payload: Record<string, unknown> = {
        user_key: STAGE_USER_KEY,
        text,
        actor_id: actorId,
        metadata: {
          port: "stage",
          channel: "stage",
        },
      };

      if (mailbox?.session?.id) {
        payload.session_id = mailbox.session.id;
      }

      const response = await fetch(
        `${baseUrl.replace(/\/+$/, "")}/ports/http`,
        {
          method: "POST",
          headers: {
            "content-type": "application/json",
          },
          body: JSON.stringify(payload),
        }
      );

      if (!response.ok) {
        const responseText = await response.text().catch(() => "");
        throw new Error(
          responseText
            ? `HTTP ${response.status}: ${responseText}`
            : `HTTP ${response.status}`
        );
      }

      await Promise.all([loadActors(), loadMailbox(actorId)]);
      requestAnimationFrame(() => {
        scrollMailboxToBottom(true);
      });
    } catch (sendError) {
      setOptimisticMessages((current) => {
        const previous = current[actorId] ?? [];
        const nextMessages = previous.filter(
          (message) => message.id !== optimisticMessageId
        );
        if (nextMessages.length === previous.length) {
          return current;
        }
        const next = { ...current };
        if (nextMessages.length > 0) {
          next[actorId] = nextMessages;
        } else {
          delete next[actorId];
        }
        return next;
      });
      setDraft(text);
      setError(errorMessage(sendError));
    } finally {
      setIsSending(false);
    }
  }, [
    baseUrl,
    draft,
    loadActors,
    loadMailbox,
    mailbox?.session?.id,
    scrollMailboxToBottom,
    selectedActorId,
  ]);

  const selectedActor = React.useMemo(
    () => actors.find((actor) => actor.id === selectedActorId) ?? null,
    [actors, selectedActorId]
  );

  const { nodes, edges } = React.useMemo(() => {
    if (actors.length === 0) {
      return { nodes: [] as Node[], edges: [] as Edge[] };
    }

    const actorIds = new Set(actors.map((actor) => actor.id));
    const userActors = new Set<string>();
    const actorPairs = new Map<string, [string, string]>();
    const actorToolGroups = new Map<string, Set<string>>();
    const toolGroups = new Set<string>();

    for (const actor of actors) {
      const messages = actorGraphMessages[actor.id] ?? [];
      for (const message of messages) {
        if (isSessionEventMessage(message)) {
          continue;
        }

        const messageType = message.messageType.toLowerCase();
        const role = (message.role ?? "").toLowerCase();
        if (
          messageType === "user" ||
          messageType === "user_audio" ||
          role === "user"
        ) {
          userActors.add(actor.id);
        }

        const toolName = extractToolName(message.payload);
        if (!toolName) {
          continue;
        }

        const toolGroup = normalizeToolGroup(toolName);
        toolGroups.add(toolGroup);
        const existingGroups = actorToolGroups.get(actor.id) ?? new Set<string>();
        existingGroups.add(toolGroup);
        actorToolGroups.set(actor.id, existingGroups);

        const normalizedToolName = toolName.toLowerCase();
        if (normalizedToolName.startsWith("actors-send")) {
          const targetActorId = extractTargetActorIdFromPayload(message.payload);
          if (
            targetActorId &&
            actorIds.has(targetActorId) &&
            targetActorId !== actor.id
          ) {
            const [left, right] = [actor.id, targetActorId].sort();
            actorPairs.set(`${left}|${right}`, [left, right]);
          }
        }
      }
    }

    const columns = Math.max(1, Math.ceil(Math.sqrt(actors.length)));
    const rows = Math.max(1, Math.ceil(actors.length / columns));
    const actorOffsetY = userActors.size > 0 ? 140 : 40;
    const actorNodes: Node[] = actors.map((actor, index) => ({
      id: actor.id,
      type: "actor",
      position: {
        x: (index % columns) * 310,
        y: Math.floor(index / columns) * 200 + actorOffsetY,
      },
      data: { actor },
      selected: actor.id === selectedActorId,
    }));

    const allNodes: Node[] = [...actorNodes];
    if (userActors.size > 0) {
      allNodes.unshift({
        id: "stage:user",
        position: {
          x: Math.max(0, ((columns - 1) * 310) / 2),
          y: 0,
        },
        data: { label: "User" },
        draggable: false,
        style: {
          borderRadius: "999px",
          border: "1px solid rgba(14, 116, 214, 0.4)",
          background: "rgba(224, 242, 254, 0.95)",
          color: "rgb(3, 105, 161)",
          fontWeight: 700,
          padding: "8px 16px",
        },
      });
    }

    const sortedToolGroups = [...toolGroups].sort((left, right) =>
      left.localeCompare(right)
    );
    if (sortedToolGroups.length > 0) {
      const totalWidth = (sortedToolGroups.length - 1) * 220;
      const startX = Math.max(0, ((columns - 1) * 310) / 2 - totalWidth / 2);
      const toolY = actorOffsetY + rows * 200 + 80;
      for (const [index, toolGroup] of sortedToolGroups.entries()) {
        allNodes.push({
          id: `stage:tool:${toolGroup}`,
          position: {
            x: startX + index * 220,
            y: toolY,
          },
          data: { label: toolGroup },
          draggable: false,
          style: {
            borderRadius: "14px",
            border: "1px solid rgba(217, 119, 6, 0.35)",
            background: "rgba(255, 247, 237, 0.96)",
            color: "rgb(146, 64, 14)",
            fontWeight: 600,
            padding: "10px 14px",
          },
        });
      }
    }

    const allEdges: Edge[] = [];
    for (const actorId of userActors) {
      allEdges.push({
        id: `stage:user:${actorId}`,
        source: "stage:user",
        target: actorId,
        animated: true,
        style: { stroke: "rgba(14, 116, 214, 0.6)", strokeWidth: 1.4 },
      });
    }

    for (const [left, right] of actorPairs.values()) {
      allEdges.push({
        id: `stage:actor:${left}:${right}`,
        source: left,
        target: right,
        style: { stroke: "rgba(71, 85, 105, 0.6)", strokeWidth: 1.4 },
      });
    }

    for (const [actorId, groups] of actorToolGroups.entries()) {
      for (const group of groups) {
        allEdges.push({
          id: `stage:tool:${actorId}:${group}`,
          source: actorId,
          target: `stage:tool:${group}`,
          style: { stroke: "rgba(217, 119, 6, 0.6)", strokeWidth: 1.3 },
          animated: true,
        });
      }
    }

    return { nodes: allNodes, edges: allEdges };
  }, [actorGraphMessages, actors, selectedActorId]);
  const selectedSession = mailbox?.session ?? null;
  const mergedMessages = React.useMemo(() => {
    const sessionMessages = selectedSession?.messages ?? [];
    const actorOptimistic = selectedActorId
      ? (optimisticMessages[selectedActorId] ?? [])
      : [];
    return [...sessionMessages, ...actorOptimistic].sort((left, right) =>
      left.createdAt.localeCompare(right.createdAt)
    );
  }, [optimisticMessages, selectedActorId, selectedSession?.messages]);

  const mailboxEntries = React.useMemo<MailboxEntry[]>(() => {
    const entries: MailboxEntry[] = [];

    for (let index = 0; index < mergedMessages.length; index += 1) {
      const message = mergedMessages[index];
      if (!message) {
        continue;
      }

      const toolName = extractToolName(message.payload);
      const isToolCall = isToolCallMessage(message) && toolName !== null;
      const isToolResult = isToolResultMessage(message) && toolName !== null;

      if (isToolCall && toolName) {
        let resultMessage: MailboxMessage | null = null;
        const nextMessage = mergedMessages[index + 1];
        if (nextMessage) {
          const nextToolName = extractToolName(nextMessage.payload);
          if (
            isToolResultMessage(nextMessage) &&
            nextToolName &&
            nextToolName === toolName
          ) {
            resultMessage = nextMessage;
            index += 1;
          }
        }

        const fieldsByKey = new Map<string, string>();
        for (const field of toToolFields(extractToolArguments(message.payload))) {
          fieldsByKey.set(field.key, field.value);
        }
        if (resultMessage) {
          for (const field of toToolFields(extractToolResult(resultMessage.payload))) {
            fieldsByKey.set(field.key, field.value);
          }
        }

        entries.push({
          kind: "tool",
          key: resultMessage
            ? `${message.id}:${resultMessage.id}`
            : `${message.id}:tool`,
          role: message.role ?? "tool",
          createdAt: message.createdAt,
          toolName: formatToolDisplayName(toolName),
          fields: [...fieldsByKey.entries()].map(([key, value]) => ({
            key,
            value,
          })),
          sourceType: "tool_call",
        });
        continue;
      }

      if (isToolResult && toolName) {
        entries.push({
          kind: "tool",
          key: `${message.id}:tool-result`,
          role: message.role ?? "tool",
          createdAt: message.createdAt,
          toolName: formatToolDisplayName(toolName),
          fields: toToolFields(extractToolResult(message.payload)),
          sourceType: "tool_result",
        });
        continue;
      }

      entries.push({
        kind: "message",
        key: message.id,
        message,
      });
    }

    return entries;
  }, [mergedMessages]);

  const messageCount = mailboxEntries.length;
  const toolCallCount = mailboxEntries.filter((entry) => entry.kind === "tool")
    .length;
  const startedAgo = formatTimeSince(selectedActor?.createdAt ?? null);

  const runtimeBadgeClass =
    runtimeStatus === "online"
      ? "bg-emerald-500/15 text-emerald-700"
      : runtimeStatus === "checking"
        ? "bg-amber-500/15 text-amber-700"
        : "bg-rose-500/15 text-rose-700";

  return (
    <div className="stage-shell h-screen w-screen overflow-hidden p-3 text-slate-900 md:p-4">
      <div className="stage-card flex h-full flex-col rounded-3xl border border-white/60 bg-white/70 shadow-xl">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-200/80 px-4 py-3">
          <div className="flex flex-row gap-3">
            <p className="text-[11px] uppercase tracking-[0.16em] text-slate-500">
              Borg Actor Playground
            </p>
            <Badge className={runtimeBadgeClass}>{runtimeStatus}</Badge>
          </div>

          <div className="flex flex-wrap items-center justify-end gap-2 text-xs">
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsCreateActorOpen(true)}
            >
              Create actor
            </Button>
          </div>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 p-3 lg:grid-cols-[1fr_32rem]">
          <section className="min-h-0 overflow-hidden rounded-2xl border border-slate-200 bg-white/85">
            {isLoadingActors && actors.length === 0 ? (
              <div className="flex h-full items-center justify-center text-sm text-slate-500">
                Loading actor graph...
              </div>
            ) : actors.length === 0 ? (
              <div className="flex h-full items-center justify-center p-8 text-center text-sm text-slate-500">
                No actors found. Create one from the top-right button.
              </div>
            ) : (
              <ReactFlow
                className="stage-flow"
                nodes={nodes}
                edges={edges}
                nodeTypes={actorNodeTypes}
                onNodeClick={(_event, node) => setSelectedActorId(node.id)}
                fitView
                fitViewOptions={{ padding: 0.2 }}
                nodesDraggable={false}
                nodesConnectable={false}
                proOptions={{ hideAttribution: true }}
              >
                <Background gap={18} size={1} />
                <MiniMap zoomable pannable nodeStrokeWidth={2} />
                <Controls position="bottom-left" />
              </ReactFlow>
            )}
          </section>

          <aside className="stage-card min-h-0 overflow-hidden rounded-2xl border border-slate-200 bg-white/90">
            {selectedActor ? (
              <div className="flex h-full min-h-0 flex-col">
                <header className="space-y-2 border-b border-slate-200 px-4 py-3">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <p className="text-[11px] uppercase tracking-[0.14em] text-slate-500">
                        Actor
                      </p>
                      <h2 className="text-sm font-semibold">{selectedActor.name}</h2>
                    </div>
                    <Badge className={statusBadgeClass(selectedActor.status)}>
                      {selectedActor.status.toLowerCase()}
                    </Badge>
                  </div>
                  <p className="truncate text-[11px] text-slate-500">
                    {selectedActor.id}
                  </p>
                </header>

                <Tabs
                  value={activeTab}
                  onValueChange={(value) => setActiveTab(value as ActorTab)}
                  className="min-h-0 flex-1"
                >
                  <div className="border-b border-slate-200 px-3 py-2">
                    <TabsList>
                      <TabsTrigger value="details">Details</TabsTrigger>
                      <TabsTrigger value="mailbox">Mailbox</TabsTrigger>
                    </TabsList>
                  </div>

                  <TabsContent value="details" className="min-h-0 flex-1">
                    <ScrollArea className="h-full px-3 py-3">
                      <div className="grid gap-2">
                        <article className="rounded-xl border border-slate-200 bg-white px-3 py-2">
                          <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                            Messages
                          </p>
                          <p className="text-xl font-semibold text-slate-900">
                            {messageCount}
                          </p>
                        </article>
                        <article className="rounded-xl border border-slate-200 bg-white px-3 py-2">
                          <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                            Tool Calls
                          </p>
                          <p className="text-xl font-semibold text-slate-900">
                            {toolCallCount}
                          </p>
                        </article>
                        <article className="rounded-xl border border-slate-200 bg-white px-3 py-2">
                          <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                            Time Since Started
                          </p>
                          <p className="text-xl font-semibold text-slate-900">
                            {startedAgo}
                          </p>
                          <p className="mt-1 text-[11px] text-slate-500">
                            Created {formatDate(selectedActor.createdAt)}
                          </p>
                        </article>
                      </div>
                    </ScrollArea>
                  </TabsContent>

                  <TabsContent value="mailbox" className="mt-0 min-h-0 flex-1">
                    <div className="flex h-full min-h-0 flex-col">
                      <div className="flex items-center justify-between gap-2 border-b border-slate-200 px-3 py-2">
                        <p className="truncate text-[11px] text-slate-500">
                          Session {selectedSession?.id ?? "(not created yet)"}
                        </p>
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() => void loadMailbox(selectedActor.id)}
                          disabled={isLoadingMailbox}
                        >
                          Refresh
                        </Button>
                      </div>

                      <ScrollArea className="min-h-0 flex-1 px-3 py-3">
                        <div className="space-y-2">
                          {isLoadingMailbox && !selectedSession ? (
                            <p className="text-xs text-slate-500">
                              Loading mailbox...
                            </p>
                          ) : mailboxEntries.length ? (
                            mailboxEntries.map((entry) => {
                              if (entry.kind === "tool") {
                                return (
                                  <article
                                    key={entry.key}
                                    className="mr-auto max-w-[94%] rounded-2xl border border-amber-300 bg-amber-50 px-3 py-2 text-xs shadow-sm"
                                  >
                                    <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-amber-800">
                                      <span>{entry.role ?? "tool"}</span>
                                      <span>{formatDate(entry.createdAt)}</span>
                                    </div>

                                    <p className="mb-1 text-[12px] font-semibold text-amber-900">
                                      {entry.toolName}
                                    </p>

                                    {entry.fields.length > 0 ? (
                                      <div className="space-y-1">
                                        {entry.fields.map((field) => (
                                          <p
                                            key={`${entry.key}:${field.key}`}
                                            className="whitespace-pre-wrap break-words text-[12px] text-amber-900"
                                          >
                                            <span className="italic">
                                              {field.key}
                                            </span>
                                            : {field.value}
                                          </p>
                                        ))}
                                      </div>
                                    ) : (
                                      <p className="text-[11px] text-amber-700">
                                        (no output)
                                      </p>
                                    )}

                                    <p className="mt-1 text-[10px] text-amber-700">
                                      {entry.sourceType}
                                    </p>
                                  </article>
                                );
                              }

                              const message = entry.message;
                              const normalizedRole = (message.role ?? "").toLowerCase();
                              const isUser = normalizedRole === "user";
                              const isAssistant = normalizedRole === "assistant";
                              const parsedPayload = parseJsonPayload(message.payload);
                              const hasText = (message.text ?? "").trim().length > 0;

                              return (
                                <article
                                  key={entry.key}
                                  className={`max-w-[94%] rounded-2xl border px-3 py-2 text-xs shadow-sm ${
                                    isUser
                                      ? "ml-auto border-sky-300 bg-sky-50"
                                      : isAssistant
                                        ? "mr-auto border-emerald-300 bg-emerald-50"
                                        : "mr-auto border-slate-300 bg-slate-50"
                                  }`}
                                >
                                  <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-slate-500">
                                    <span>{message.role ?? "system"}</span>
                                    <span>{formatDate(message.createdAt)}</span>
                                  </div>

                                  {hasText ? (
                                    <p className="whitespace-pre-wrap text-[12px] text-slate-700">
                                      {message.text}
                                    </p>
                                  ) : parsedPayload !== null && parsedPayload !== undefined ? (
                                    typeof parsedPayload === "object" ? (
                                      <div className="rounded-lg border border-slate-200 bg-white p-2">
                                        <JsonTreeViewer
                                          value={parsedPayload}
                                          defaultExpandedDepth={1}
                                        />
                                      </div>
                                    ) : (
                                      <p className="whitespace-pre-wrap text-[12px] text-slate-700">
                                        {String(parsedPayload)}
                                      </p>
                                    )
                                  ) : (
                                    <p className="text-[12px] text-slate-500">
                                      (empty payload)
                                    </p>
                                  )}

                                  <p className="mt-1 text-[10px] text-slate-500">
                                    {message.messageType}
                                  </p>
                                </article>
                              );
                            })
                          ) : (
                            <p className="text-xs text-slate-500">
                              No messages for this actor yet.
                            </p>
                          )}
                          <div ref={mailboxEndRef} />
                        </div>
                      </ScrollArea>

                      <footer className="space-y-2 border-t border-slate-200 px-3 py-3">
                        <Input
                          value={draft}
                          onChange={(event) => setDraft(event.currentTarget.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" && !event.shiftKey) {
                              event.preventDefault();
                              void sendMessage();
                            }
                          }}
                          placeholder="Type a message to this actor"
                        />
                        <div className="flex items-center justify-between gap-2">
                          <p className="text-[11px] text-slate-500">
                            Sends to <code>/ports/http</code> with actor binding.
                          </p>
                          <Button
                            type="button"
                            onClick={() => void sendMessage()}
                            disabled={isSending || draft.trim().length === 0}
                          >
                            {isSending ? "Sending..." : "Send"}
                          </Button>
                        </div>
                      </footer>
                    </div>
                  </TabsContent>
                </Tabs>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center p-6 text-center text-sm text-slate-500">
                Select an actor node to inspect mailbox activity.
              </div>
            )}
          </aside>
        </div>

        {error ? (
          <div className="mx-3 mb-3 rounded-xl border border-rose-300 bg-rose-50 px-3 py-2 text-xs text-rose-700">
            {error}
          </div>
        ) : null}
      </div>

      <Dialog
        open={isCreateActorOpen}
        onOpenChange={(open) => {
          if (!isCreatingActor) {
            setIsCreateActorOpen(open);
          }
        }}
      >
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Create Actor</DialogTitle>
            <DialogDescription>
              Define the actor id, status, and system prompt used by the runtime.
            </DialogDescription>
          </DialogHeader>

          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              void createActor();
            }}
          >
            <div className="space-y-2">
              <Label htmlFor="stage-create-actor-name">Name</Label>
              <Input
                id="stage-create-actor-name"
                value={createActorDraft.name}
                onChange={(event) =>
                  setCreateActorDraft((current) => ({
                    ...current,
                    name: event.currentTarget.value,
                  }))
                }
                placeholder="Planner"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="stage-create-actor-id">Actor ID (optional)</Label>
              <Input
                id="stage-create-actor-id"
                value={createActorDraft.actorId}
                onChange={(event) =>
                  setCreateActorDraft((current) => ({
                    ...current,
                    actorId: event.currentTarget.value,
                  }))
                }
                placeholder="borg:actor:planner"
              />
              <p className="text-[11px] text-slate-500">
                Resolved id: <code>{createActorIdPreview || "(missing)"}</code>
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="stage-create-actor-status">Status</Label>
              <Select
                value={createActorDraft.status}
                onValueChange={(value) =>
                  setCreateActorDraft((current) => ({
                    ...current,
                    status: value as ActorStatusValue,
                  }))
                }
              >
                <SelectTrigger id="stage-create-actor-status" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CREATE_ACTOR_STATUS_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label htmlFor="stage-create-actor-prompt">System Prompt</Label>
              <Textarea
                id="stage-create-actor-prompt"
                value={createActorDraft.systemPrompt}
                onChange={(event) =>
                  setCreateActorDraft((current) => ({
                    ...current,
                    systemPrompt: event.currentTarget.value,
                  }))
                }
                className="min-h-32"
                placeholder="You are an engineering actor specialized in code execution and review."
              />
            </div>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={isCreatingActor}
                onClick={() => setIsCreateActorOpen(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isCreatingActor}>
                {isCreatingActor ? "Creating..." : "Create actor"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
