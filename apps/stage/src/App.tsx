import {
  ActorStatusValue,
  requestGraphQL,
  resolveDefaultBaseUrl,
} from "@borg/graphql-client";
import {
  Badge,
  Button,
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
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
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
} from "@borg/ui";
import React from "react";
import ReactFlow, {
  applyNodeChanges,
  Background,
  Controls,
  type Edge,
  Handle,
  MiniMap,
  type Node,
  type NodeChange,
  type NodeProps,
  type NodeTypes,
  Position,
} from "reactflow";

const STAGE_QUERY_ACTORS = `
  query StageActors($first: Int!) {
    actors(first: $first) {
      edges {
        node {
          id
          name
          model
          systemPrompt
          status
          createdAt
          updatedAt
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

const STAGE_MUTATION_DELETE_ACTOR = `
  mutation StageDeleteActor($id: Uri!) {
    deleteActor(id: $id)
  }
`;

const STAGE_MUTATION_UPSERT_PORT = `
  mutation StageUpsertPort($input: UpsertPortInput!) {
    upsertPort(input: $input) {
      id
      name
      provider
      enabled
      allowsGuests
      assignedActorId
      settings
    }
  }
`;

const STAGE_MUTATION_UPSERT_PORT_ACTOR_BINDING = `
  mutation StageUpsertPortActorBinding($input: UpsertPortActorBindingInput!) {
    upsertPortActorBinding(input: $input) {
      id
      conversationKey
      actorId
    }
  }
`;

const STAGE_QUERY_PROVIDERS = `
  query StageProviders($first: Int!) {
    providers(first: $first) {
      edges {
        node {
          id
          provider
          providerKind
          enabled
          defaultTextModel
          defaultModel {
            name
          }
          models {
            name
          }
        }
      }
    }
  }
`;

const STAGE_QUERY_PORTS = `
  query StagePorts($first: Int!, $bindingsFirst: Int!, $actorBindingsFirst: Int = 200) {
    ports(first: $first) {
      edges {
        node {
          id
          name
          provider
          enabled
          allowsGuests
          assignedActorId
          settings
          bindings(first: $bindingsFirst) {
            edges {
              node {
                id
                conversationKey
                actorId
              }
            }
          }
          actorBindings(first: $actorBindingsFirst) {
            edges {
              node {
                id
                conversationKey
                actorId
              }
            }
          }
        }
      }
    }
  }
`;

type RuntimeStatus = "checking" | "online" | "offline";

type ActorSummary = {
  id: string;
  name: string;
  systemPrompt: string;
  provider: string;
  model: string;
  status: string;
  createdAt: string;
  updatedAt: string;
};

type MailboxMessage = {
  id: string;
  createdAt: string;
  messageType: string;
  role: string | null;
  text: string | null;
  payload: unknown;
};

type ActorMailbox = {
  actorId: string;
  actorName: string;
  actorStatus: string;
  messages: MailboxMessage[];
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
        model: string | null;
        systemPrompt: string;
        status: string;
        createdAt: string;
        updatedAt: string;
      };
    }>;
  };
};

type StageActorMailboxResponse = {
  actor: {
    id: string;
    name: string;
    status: string;
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
  } | null;
};

type StageUpsertActorResponse = {
  upsertActor: {
    id: string;
    name: string;
    status: string;
  };
};

type StageDeleteActorResponse = {
  deleteActor: boolean;
};

type StageUpsertPortResponse = {
  upsertPort: {
    id: string;
    name: string;
    provider: string;
    enabled: boolean;
    allowsGuests: boolean;
    assignedActorId: string | null;
    settings: unknown;
  };
};

type StageUpsertPortActorBindingResponse = {
  upsertPortActorBinding: {
    id: string;
    conversationKey: string;
    actorId: string | null;
  };
};

type StageProvidersResponse = {
  providers: {
    edges: Array<{
      node: {
        id: string;
        provider: string;
        providerKind: string;
        enabled: boolean;
        defaultTextModel: string | null;
        defaultModel: {
          name: string;
        } | null;
        models: Array<{
          name: string;
        }>;
      };
    }>;
  };
};

type ProviderInfo = {
  id: string;
  provider: string;
  providerKind: string;
  enabled: boolean;
  defaultTextModel: string | null;
  defaultModel: string | null;
  models: string[];
};

type PortSummary = {
  id: string;
  name: string;
  provider: string;
  enabled: boolean;
  allowsGuests: boolean;
  assignedActorId: string | null;
  settings: unknown;
  bindings: Array<{
    id: string;
    conversationKey: string;
    actorId: string;
  }>;
  actorBindings: Array<{
    id: string;
    conversationKey: string;
    actorId: string | null;
  }>;
  actorIds: string[];
};

type StagePortsResponse = {
  ports: {
    edges: Array<{
      node: {
        id: string;
        name: string;
        provider: string;
        enabled: boolean;
        allowsGuests: boolean;
        assignedActorId: string | null;
        settings: unknown;
        bindings: {
          edges: Array<{
            node: {
              id: string;
              conversationKey: string;
              actorId: string;
            };
          }>;
        };
        actorBindings: {
          edges: Array<{
            node: {
              id: string;
              conversationKey: string;
              actorId: string | null;
            };
          }>;
        };
      };
    }>;
  };
};

type ActorNodeData = {
  actor: ActorSummary;
  onToggleStatus: (actorId: string, newStatus: string) => void;
};

type ActorTab = "details" | "mailbox";
type ActorDetailsDraft = {
  name: string;
  provider: string;
  model: string;
  status: ActorStatusValue;
  systemPrompt: string;
};

type PortDetailsDraft = {
  name: string;
  provider: string;
  enabled: boolean;
  allowsGuests: boolean;
  assignedActorId: string;
};

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
type ToolMailboxEntry = Extract<MailboxEntry, { kind: "tool" }>;

const DEFAULT_CREATE_ACTOR_DRAFT: CreateActorDraft = {
  actorId: "",
  name: "",
  provider: "",
  model: "",
  status: ActorStatusValue.Running,
  systemPrompt: "You are a pragmatic actor. Be precise and action-oriented.",
};

function formatDate(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return "Unknown";
  }
  return parsed.toLocaleString();
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

const RUNTIME_PROVIDER_HINT = "[runtime.provider]";
const RUNTIME_MODEL_HINT = "[runtime.model]";

function parseActorStatusValue(status: string): ActorStatusValue {
  const normalized = status.toUpperCase();
  switch (normalized) {
    case "PAUSED":
      return ActorStatusValue.Paused;
    case "DISABLED":
      return ActorStatusValue.Disabled;
    case "ERROR":
      return ActorStatusValue.Error;
    default:
      return ActorStatusValue.Running;
  }
}

function normalizeComboboxValue(value: string | null | undefined): string {
  return typeof value === "string" ? value : "";
}

function parseRuntimeHintsFromPrompt(rawPrompt: string): {
  provider: string;
  model: string;
  prompt: string;
} {
  let provider = "";
  let model = "";
  const lines = rawPrompt.split(/\r?\n/);
  const promptLines: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    const lowered = trimmed.toLowerCase();
    if (lowered.startsWith(RUNTIME_PROVIDER_HINT)) {
      provider = trimmed.slice(RUNTIME_PROVIDER_HINT.length).trim();
      continue;
    }
    if (lowered.startsWith(RUNTIME_MODEL_HINT)) {
      model = trimmed.slice(RUNTIME_MODEL_HINT.length).trim();
      continue;
    }
    promptLines.push(line);
  }

  while (promptLines.length > 0 && promptLines[0]?.trim() === "") {
    promptLines.shift();
  }
  while (
    promptLines.length > 0 &&
    promptLines[promptLines.length - 1]?.trim() === ""
  ) {
    promptLines.pop();
  }

  return {
    provider,
    model,
    prompt: promptLines.join("\n"),
  };
}

function buildPromptWithRuntimeHints(
  provider: string,
  model: string,
  systemPrompt: string
): string {
  const trimmedProvider = provider.trim();
  const trimmedModel = model.trim();
  const trimmedPrompt = systemPrompt.trim();

  const lines: string[] = [];
  if (trimmedProvider) {
    lines.push(`${RUNTIME_PROVIDER_HINT} ${trimmedProvider}`);
  }
  if (trimmedModel) {
    lines.push(`${RUNTIME_MODEL_HINT} ${trimmedModel}`);
  }
  if (lines.length > 0 && trimmedPrompt.length > 0) {
    lines.push("");
  }
  if (trimmedPrompt.length > 0) {
    lines.push(trimmedPrompt);
  }
  return lines.join("\n");
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

function stageUserKeyForActor(actorId: string): string {
  const suffix = slugifyActorSegment(actorId);
  return `borg:user:stage:${suffix || "actor"}`;
}

function colorForProvider(provider: string): string {
  const normalized = provider.trim().toLowerCase();
  if (normalized === "openai") {
    return "#0f766e";
  }
  if (normalized === "openrouter") {
    return "#1d4ed8";
  }
  if (normalized.length === 0) {
    return "#64748b";
  }

  const palette = [
    "#0f766e",
    "#1d4ed8",
    "#be185d",
    "#7c3aed",
    "#0369a1",
    "#b45309",
    "#0e7490",
    "#9333ea",
  ];
  let hash = 0;
  for (let index = 0; index < normalized.length; index += 1) {
    hash = (hash << 5) - hash + normalized.charCodeAt(index);
    hash |= 0;
  }
  return palette[Math.abs(hash) % palette.length] ?? "#64748b";
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

function formatPortClientLabel(conversationKey: string): string {
  const trimmed = conversationKey.trim();
  if (!trimmed) {
    return "client";
  }

  const parts = trimmed.split(":");
  if (parts.length >= 3) {
    return `${parts[1]}:${parts.slice(2).join(":")}`;
  }

  return trimmed;
}

function isActorEventMessage(message: MailboxMessage): boolean {
  return message.messageType.toLowerCase() === "actor_event";
}

function isActorsSendMessage(message: MailboxMessage): boolean {
  const toolName = extractToolName(message.payload);
  if (!toolName) {
    return false;
  }
  return toolName.toLowerCase().startsWith("actors-send");
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

const TOOL_COLLAPSE_FIELD_COUNT = 4;
const TOOL_COLLAPSE_TOTAL_CHAR_COUNT = 280;
const TOOL_COLLAPSE_SINGLE_VALUE_CHAR_COUNT = 180;

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

function truncateMiddle(value: string, maxChars: number): string {
  if (value.length <= maxChars) {
    return value;
  }
  const half = Math.floor((maxChars - 3) / 2);
  return `${value.slice(0, half)}...${value.slice(value.length - half)}`;
}

function shouldCollapseToolEntry(entry: ToolMailboxEntry): boolean {
  if (entry.fields.length === 0) {
    return false;
  }

  const totalChars = entry.fields.reduce(
    (sum, field) => sum + field.key.length + field.value.length,
    0
  );
  const longestValue = entry.fields.reduce(
    (maxChars, field) => Math.max(maxChars, field.value.length),
    0
  );

  if (entry.sourceType === "tool_result") {
    return (
      entry.fields.length > 2 ||
      totalChars > TOOL_COLLAPSE_TOTAL_CHAR_COUNT / 2 ||
      longestValue > TOOL_COLLAPSE_SINGLE_VALUE_CHAR_COUNT / 2
    );
  }

  return (
    entry.fields.length > TOOL_COLLAPSE_FIELD_COUNT ||
    totalChars > TOOL_COLLAPSE_TOTAL_CHAR_COUNT ||
    longestValue > TOOL_COLLAPSE_SINGLE_VALUE_CHAR_COUNT
  );
}

function summarizeToolEntry(entry: ToolMailboxEntry): string {
  if (entry.fields.length === 0) {
    return "(no output)";
  }

  const [firstField] = entry.fields;
  if (!firstField) {
    return "(no output)";
  }

  const first = `${firstField.key}: ${truncateMiddle(firstField.value, 96)}`;
  if (entry.fields.length === 1) {
    return first;
  }
  return `${first} (+${entry.fields.length - 1} fields)`;
}

function ActorNodeCard({ data, selected }: NodeProps<ActorNodeData>) {
  const actor = data.actor;
  const onToggleStatus = data.onToggleStatus;
  const isRunning = actor.status.toUpperCase() === "RUNNING";
  const hasRuntimeConfig =
    actor.provider.trim().length > 0 && actor.model.trim().length > 0;

  return (
    <div
      className={`stage-card w-48 rounded-2xl border px-3 py-2 shadow-md transition ${
        selected
          ? "border-sky-400 bg-sky-50"
          : "border-slate-200 bg-white/90 hover:border-slate-300"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={`h-2.5 w-2.5 shrink-0 rounded-full ${
              isRunning ? "bg-emerald-500" : "bg-slate-400"
            }`}
          />
          <p className="truncate text-xs font-semibold">{actor.name}</p>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleStatus(actor.id, isRunning ? "PAUSED" : "RUNNING");
            }}
            className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${
              isRunning
                ? "bg-amber-100 text-amber-700 hover:bg-amber-200"
                : "bg-emerald-100 text-emerald-700 hover:bg-emerald-200"
            }`}
          >
            {isRunning ? "⏸" : "▶"}
          </button>
        </div>
      </div>
      <p className="mt-1 truncate text-[11px] text-slate-600">
        {hasRuntimeConfig
          ? `${actor.provider} / ${actor.model}`
          : "provider/model not set"}
      </p>
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2 !w-2 !opacity-0"
      />
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2 !w-2 !opacity-0"
      />
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
  const [isLoadingActors, setIsLoadingActors] = React.useState(true);
  const [isLoadingMailbox, setIsLoadingMailbox] = React.useState(false);
  const [isSending, setIsSending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const [selectedActorId, setSelectedActorId] = React.useState<string | null>(
    null
  );
  const [selectedPortId, setSelectedPortId] = React.useState<string | null>(
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
  const [isDeletingActor, setIsDeletingActor] = React.useState(false);
  const [ports, setPorts] = React.useState<PortSummary[]>([]);
  const [providers, setProviders] = React.useState<ProviderInfo[]>([]);
  const [availableModels, setAvailableModels] = React.useState<string[]>([]);
  const [createActorDraft, setCreateActorDraft] = React.useState(
    DEFAULT_CREATE_ACTOR_DRAFT
  );
  const [pinnedActorPositions, setPinnedActorPositions] = React.useState<
    Record<string, { x: number; y: number }>
  >({});
  const [expandedToolEntries, setExpandedToolEntries] = React.useState<
    Record<string, boolean>
  >({});
  const [actorDetailsDraft, setActorDetailsDraft] =
    React.useState<ActorDetailsDraft | null>(null);
  const [isSavingActorDetails, setIsSavingActorDetails] = React.useState(false);
  const [portDetailsDraft, setPortDetailsDraft] =
    React.useState<PortDetailsDraft | null>(null);
  const [portBindingDrafts, setPortBindingDrafts] = React.useState<
    Record<string, string>
  >({});
  const [isSavingPortDetails, setIsSavingPortDetails] = React.useState(false);
  const [savingBindingKey, setSavingBindingKey] = React.useState<string | null>(
    null
  );

  const baseUrl = React.useMemo(() => resolveDefaultBaseUrl(), []);
  const mailboxEndRef = React.useRef<HTMLDivElement | null>(null);
  const sendInFlightRef = React.useRef(false);
  const selectedActor = React.useMemo(
    () => actors.find((actor) => actor.id === selectedActorId) ?? null,
    [actors, selectedActorId]
  );
  const selectedPort = React.useMemo(
    () => ports.find((port) => port.id === selectedPortId) ?? null,
    [ports, selectedPortId]
  );

  const scrollMailboxToBottom = React.useCallback((smooth = true) => {
    mailboxEndRef.current?.scrollIntoView({
      behavior: smooth ? "smooth" : "auto",
      block: "end",
    });
  }, []);

  const loadActorGraphMessages = React.useCallback(
    async (actorIds: string[]) => {
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
              data.actor?.messages.edges
                .map((edge) => edge.node)
                .map((message) => ({
                  id: message.id,
                  createdAt: message.createdAt,
                  messageType: message.messageType,
                  role: message.role,
                  text: message.text,
                  payload: message.payload,
                }))
                .filter((message) => !isActorEventMessage(message)) ?? [];
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
    },
    []
  );

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
        .map((node) => {
          const runtimeHints = parseRuntimeHintsFromPrompt(node.systemPrompt);
          return {
            id: node.id,
            name: node.name,
            systemPrompt: node.systemPrompt,
            provider: runtimeHints.provider,
            model: (node.model ?? "").trim(),
            status: node.status,
            createdAt: node.createdAt,
            updatedAt: node.updatedAt,
          };
        });

      setActors(nextActors);
      void loadActorGraphMessages(nextActors.map((actor) => actor.id));
      setRuntimeStatus("online");

      setSelectedActorId((previous) => {
        if (selectedPortId) {
          return null;
        }
        if (!previous) {
          return nextActors[0]?.id ?? null;
        }
        return nextActors.some((actor) => actor.id === previous)
          ? previous
          : (nextActors[0]?.id ?? null);
      });
    } catch (loadError) {
      setRuntimeStatus("offline");
      setError(errorMessage(loadError));
    } finally {
      setIsLoadingActors(false);
    }
  }, [loadActorGraphMessages, selectedPortId]);

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

      const messages = data.actor.messages.edges
        .map((messageEdge) => messageEdge.node)
        .map((message) => ({
          id: message.id,
          createdAt: message.createdAt,
          messageType: message.messageType,
          role: message.role,
          text: message.text,
          payload: message.payload,
        }))
        .filter((message) => !isActorEventMessage(message))
        .sort((left, right) => left.createdAt.localeCompare(right.createdAt));

      setMailbox({
        actorId: data.actor.id,
        actorName: data.actor.name,
        actorStatus: data.actor.status,
        messages,
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

  const loadProviders = React.useCallback(async () => {
    try {
      const data = await requestGraphQL<
        StageProvidersResponse,
        { first: number }
      >({
        query: STAGE_QUERY_PROVIDERS,
        variables: { first: 50 },
      });

      const providerList: ProviderInfo[] = data.providers.edges
        .map((edge) => edge.node)
        .filter((p) => p.enabled)
        .map((p) => {
          const modelSet = new Set<string>();
          for (const model of p.models ?? []) {
            if (model.name.trim().length > 0) {
              modelSet.add(model.name.trim());
            }
          }
          if (p.defaultModel?.name?.trim()) {
            modelSet.add(p.defaultModel.name.trim());
          }
          if (p.defaultTextModel?.trim()) {
            modelSet.add(p.defaultTextModel.trim());
          }
          return {
            id: p.id,
            provider: p.provider,
            providerKind: p.providerKind,
            enabled: p.enabled,
            defaultTextModel: p.defaultTextModel,
            defaultModel: p.defaultModel?.name ?? null,
            models: Array.from(modelSet).sort(),
          };
        });

      setProviders(providerList);
    } catch (err) {
      console.error("Failed to load providers:", err);
    }
  }, []);

  const loadPorts = React.useCallback(async () => {
    try {
      const data = await requestGraphQL<
        StagePortsResponse,
        { first: number; bindingsFirst: number; actorBindingsFirst: number }
      >({
        query: STAGE_QUERY_PORTS,
        variables: { first: 50, bindingsFirst: 200, actorBindingsFirst: 200 },
      });

      const nextPorts: PortSummary[] = data.ports.edges.map((edge) => {
        const node = edge.node;
        const actorIds = new Set<string>();
        const bindings = node.bindings.edges.map((binding) => {
          actorIds.add(binding.node.actorId);
          return {
            id: binding.node.id,
            conversationKey: binding.node.conversationKey,
            actorId: binding.node.actorId,
          };
        });
        const actorBindings = node.actorBindings.edges.map((binding) => {
          if (binding.node.actorId) {
            actorIds.add(binding.node.actorId);
          }
          return {
            id: binding.node.id,
            conversationKey: binding.node.conversationKey,
            actorId: binding.node.actorId,
          };
        });

        if (node.assignedActorId) {
          actorIds.add(node.assignedActorId);
        }

        return {
          id: node.id,
          name: node.name,
          provider: node.provider,
          enabled: node.enabled,
          allowsGuests: node.allowsGuests,
          assignedActorId: node.assignedActorId,
          settings: node.settings,
          bindings,
          actorBindings,
          actorIds: Array.from(actorIds),
        };
      });

      setPorts(nextPorts);
    } catch (err) {
      console.error("Failed to load ports:", err);
    }
  }, []);

  const createActor = React.useCallback(async () => {
    const actorId = normalizeActorId(
      createActorDraft.actorId,
      createActorDraft.name
    );
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

    const systemPromptWithRuntimeHints = buildPromptWithRuntimeHints(
      provider,
      model,
      systemPrompt
    );

    setIsCreatingActor(true);
    setError(null);

    try {
      await requestGraphQL<
        StageUpsertActorResponse,
        {
          input: {
            id: string;
            name: string;
            model?: string;
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
            model,
            status: createActorDraft.status,
            systemPrompt: systemPromptWithRuntimeHints,
          },
        },
      });

      setIsCreateActorOpen(false);
      setCreateActorDraft(DEFAULT_CREATE_ACTOR_DRAFT);
      setSelectedActorId(actorId);
      setSelectedPortId(null);
      setActiveTab("mailbox");

      await Promise.all([loadActors(), loadMailbox(actorId)]);
    } catch (createError) {
      setError(errorMessage(createError));
    } finally {
      setIsCreatingActor(false);
    }
  }, [createActorDraft, loadActors, loadMailbox]);

  const deleteActor = React.useCallback(async () => {
    if (!selectedActorId) {
      return;
    }
    const actor = actors.find((entry) => entry.id === selectedActorId);
    if (!actor) {
      return;
    }

    const actorId = actor.id;
    setIsDeletingActor(true);
    setError(null);

    try {
      const data = await requestGraphQL<
        StageDeleteActorResponse,
        { id: string }
      >({
        query: STAGE_MUTATION_DELETE_ACTOR,
        variables: {
          id: actorId,
        },
      });

      if (!data.deleteActor) {
        throw new Error("Actor deletion failed.");
      }

      setMailbox(null);
      setOptimisticMessages((current) => {
        if (!current[actorId]) {
          return current;
        }
        const next = { ...current };
        delete next[actorId];
        return next;
      });
      setActorGraphMessages((current) => {
        if (!current[actorId]) {
          return current;
        }
        const next = { ...current };
        delete next[actorId];
        return next;
      });

      await loadActors();
    } catch (deleteError) {
      setError(errorMessage(deleteError));
    } finally {
      setIsDeletingActor(false);
    }
  }, [actors, loadActors, selectedActorId]);

  React.useEffect(() => {
    void loadActors();
    void loadPorts();

    const timer = setInterval(() => {
      void loadActors();
      void loadPorts();
    }, 12000);

    return () => clearInterval(timer);
  }, [loadActors, loadPorts]);

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

  React.useEffect(() => {
    setExpandedToolEntries({});
  }, [selectedActorId]);

  React.useEffect(() => {
    if (providers.length === 0) {
      void loadProviders();
    }
  }, [providers.length, loadProviders]);

  React.useEffect(() => {
    if (!selectedActor) {
      setActorDetailsDraft(null);
      return;
    }

    const parsedPrompt = parseRuntimeHintsFromPrompt(
      selectedActor.systemPrompt
    );
    setActorDetailsDraft({
      name: selectedActor.name,
      provider: parsedPrompt.provider || selectedActor.provider,
      model: selectedActor.model,
      status: parseActorStatusValue(selectedActor.status),
      systemPrompt: parsedPrompt.prompt,
    });
  }, [selectedActor]);

  React.useEffect(() => {
    if (!selectedPort) {
      setPortDetailsDraft(null);
      setPortBindingDrafts({});
      return;
    }

    setPortDetailsDraft({
      name: selectedPort.name,
      provider: selectedPort.provider,
      enabled: selectedPort.enabled,
      allowsGuests: selectedPort.allowsGuests,
      assignedActorId: selectedPort.assignedActorId ?? "",
    });

    const nextBindingDrafts: Record<string, string> = {};
    for (const binding of selectedPort.actorBindings) {
      nextBindingDrafts[binding.id] = binding.actorId ?? "";
    }
    setPortBindingDrafts(nextBindingDrafts);
  }, [selectedPort]);

  React.useEffect(() => {
    if (providers.length === 0) {
      setAvailableModels([]);
      return;
    }

    const selectedProvider = providers.find(
      (provider) => provider.provider === createActorDraft.provider
    );
    const nextModels =
      selectedProvider?.models ??
      Array.from(
        new Set(providers.flatMap((provider) => provider.models))
      ).sort();

    setAvailableModels(nextModels);

    if (!selectedProvider) {
      return;
    }

    const preferredModel = selectedProvider.defaultModel ?? nextModels[0] ?? "";
    const currentModel = createActorDraft.model.trim();
    if (
      preferredModel &&
      (currentModel.length === 0 || !nextModels.includes(currentModel))
    ) {
      setCreateActorDraft((current) => ({
        ...current,
        model: preferredModel,
      }));
    }
  }, [providers, createActorDraft.provider, createActorDraft.model]);

  React.useEffect(() => {
    if (!actorDetailsDraft || providers.length === 0) {
      return;
    }

    const selectedProvider = providers.find(
      (provider) => provider.provider === actorDetailsDraft.provider
    );
    const nextModels =
      selectedProvider?.models ??
      Array.from(
        new Set(providers.flatMap((provider) => provider.models))
      ).sort();
    const preferredModel =
      selectedProvider?.defaultModel ?? nextModels[0] ?? "";
    const currentModel = actorDetailsDraft.model.trim();

    if (
      preferredModel &&
      (currentModel.length === 0 || !nextModels.includes(currentModel))
    ) {
      setActorDetailsDraft((current) =>
        current
          ? {
              ...current,
              model: preferredModel,
            }
          : current
      );
    }
  }, [
    actorDetailsDraft?.model,
    actorDetailsDraft?.provider,
    providers,
    actorDetailsDraft,
  ]);

  const sendMessage = React.useCallback(async () => {
    if (sendInFlightRef.current) {
      return;
    }

    const text = draft.trim();
    if (!text || !selectedActorId) {
      return;
    }

    sendInFlightRef.current = true;
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
        user_key: stageUserKeyForActor(actorId),
        text,
        actor_id: actorId,
        metadata: {
          port: "stage",
          channel: "stage",
        },
      };

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
      sendInFlightRef.current = false;
      setIsSending(false);
    }
  }, [
    baseUrl,
    draft,
    loadActors,
    loadMailbox,
    scrollMailboxToBottom,
    selectedActorId,
  ]);

  const detailsModelOptions = React.useMemo(() => {
    if (providers.length === 0) {
      return [] as string[];
    }
    const selectedProvider = providers.find(
      (provider) => provider.provider === actorDetailsDraft?.provider
    );
    return (
      selectedProvider?.models ??
      Array.from(
        new Set(providers.flatMap((provider) => provider.models))
      ).sort()
    );
  }, [actorDetailsDraft?.provider, providers]);

  const availablePortProviders = React.useMemo(() => {
    const values = new Set<string>();
    for (const port of ports) {
      const provider = port.provider.trim();
      if (provider.length > 0) {
        values.add(provider);
      }
    }
    const selectedProvider = selectedPort?.provider.trim() ?? "";
    if (selectedProvider.length > 0) {
      values.add(selectedProvider);
    }
    return Array.from(values).sort((left, right) => left.localeCompare(right));
  }, [ports, selectedPort?.provider]);

  const actorDetailsDirty = React.useMemo(() => {
    if (!selectedActor || !actorDetailsDraft) {
      return false;
    }
    const runtimeHints = parseRuntimeHintsFromPrompt(
      selectedActor.systemPrompt
    );
    const baseline = {
      name: selectedActor.name.trim(),
      provider: (runtimeHints.provider || selectedActor.provider).trim(),
      model: selectedActor.model.trim(),
      status: parseActorStatusValue(selectedActor.status),
      systemPrompt: runtimeHints.prompt.trim(),
    };
    const current = {
      name: actorDetailsDraft.name.trim(),
      provider: actorDetailsDraft.provider.trim(),
      model: actorDetailsDraft.model.trim(),
      status: actorDetailsDraft.status,
      systemPrompt: actorDetailsDraft.systemPrompt.trim(),
    };
    return (
      current.name !== baseline.name ||
      current.provider !== baseline.provider ||
      current.model !== baseline.model ||
      current.status !== baseline.status ||
      current.systemPrompt !== baseline.systemPrompt
    );
  }, [actorDetailsDraft, selectedActor]);

  const portDetailsDirty = React.useMemo(() => {
    if (!selectedPort || !portDetailsDraft) {
      return false;
    }
    return (
      portDetailsDraft.name.trim() !== selectedPort.name.trim() ||
      portDetailsDraft.provider.trim() !== selectedPort.provider.trim() ||
      portDetailsDraft.enabled !== selectedPort.enabled ||
      portDetailsDraft.allowsGuests !== selectedPort.allowsGuests ||
      portDetailsDraft.assignedActorId.trim() !==
        (selectedPort.assignedActorId ?? "").trim()
    );
  }, [portDetailsDraft, selectedPort]);

  const dirtyPortBindingIds = React.useMemo(() => {
    if (!selectedPort) {
      return new Set<string>();
    }
    const dirty = new Set<string>();
    for (const binding of selectedPort.actorBindings) {
      const current = (portBindingDrafts[binding.id] ?? "").trim();
      const baseline = (binding.actorId ?? "").trim();
      if (current !== baseline) {
        dirty.add(binding.id);
      }
    }
    return dirty;
  }, [portBindingDrafts, selectedPort]);

  const saveActorDetails = React.useCallback(async () => {
    if (!selectedActor || !actorDetailsDraft) {
      return;
    }

    const name = actorDetailsDraft.name.trim();
    const systemPrompt = actorDetailsDraft.systemPrompt.trim();
    const provider = actorDetailsDraft.provider.trim();
    const model = actorDetailsDraft.model.trim();

    if (!name) {
      setError("Actor name cannot be empty.");
      return;
    }
    if (!systemPrompt) {
      setError("System prompt cannot be empty.");
      return;
    }
    if ((provider && !model) || (!provider && model)) {
      setError("Provider and model must be set together.");
      return;
    }

    setIsSavingActorDetails(true);
    setError(null);

    try {
      const promptWithHints = buildPromptWithRuntimeHints(
        provider,
        model,
        systemPrompt
      );
      await requestGraphQL<
        StageUpsertActorResponse,
        {
          input: {
            id: string;
            name: string;
            model?: string;
            status: ActorStatusValue;
            systemPrompt: string;
          };
        }
      >({
        query: STAGE_MUTATION_UPSERT_ACTOR,
        variables: {
          input: {
            id: selectedActor.id,
            name,
            model: model || undefined,
            status: actorDetailsDraft.status,
            systemPrompt: promptWithHints,
          },
        },
      });

      await Promise.all([loadActors(), loadMailbox(selectedActor.id)]);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSavingActorDetails(false);
    }
  }, [actorDetailsDraft, loadActors, loadMailbox, selectedActor]);

  const savePortDetails = React.useCallback(async () => {
    if (!selectedPort || !portDetailsDraft) {
      return;
    }

    const provider = portDetailsDraft.provider.trim();
    if (!provider) {
      setError("Port provider cannot be empty.");
      return;
    }

    const name = portDetailsDraft.name.trim();
    if (!name) {
      setError("Port name cannot be empty.");
      return;
    }

    setIsSavingPortDetails(true);
    setError(null);
    try {
      await requestGraphQL<
        StageUpsertPortResponse,
        {
          input: {
            name: string;
            provider: string;
            enabled: boolean;
            allowsGuests: boolean;
            assignedActorId: string | null;
            settings: unknown;
          };
        }
      >({
        query: STAGE_MUTATION_UPSERT_PORT,
        variables: {
          input: {
            name: name,
            provider,
            enabled: portDetailsDraft.enabled,
            allowsGuests: portDetailsDraft.allowsGuests,
            assignedActorId: portDetailsDraft.assignedActorId || null,
            settings: selectedPort.settings ?? {},
          },
        },
      });
      await loadPorts();
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSavingPortDetails(false);
    }
  }, [loadPorts, portDetailsDraft, selectedPort]);

  const savePortBinding = React.useCallback(
    async (bindingId: string) => {
      if (!selectedPort) {
        return;
      }
      const binding = selectedPort.actorBindings.find(
        (row) => row.id === bindingId
      );
      if (!binding) {
        return;
      }

      setSavingBindingKey(bindingId);
      setError(null);
      try {
        await requestGraphQL<
          StageUpsertPortActorBindingResponse,
          {
            input: {
              portName: string;
              conversationKey: string;
              actorId: string | null;
            };
          }
        >({
          query: STAGE_MUTATION_UPSERT_PORT_ACTOR_BINDING,
          variables: {
            input: {
              portName: selectedPort.name,
              conversationKey: binding.conversationKey,
              actorId: portBindingDrafts[bindingId] || null,
            },
          },
        });
        await loadPorts();
      } catch (saveError) {
        setError(errorMessage(saveError));
      } finally {
        setSavingBindingKey(null);
      }
    },
    [loadPorts, portBindingDrafts, selectedPort]
  );

  const actorIdSet = React.useMemo(
    () => new Set(actors.map((actor) => actor.id)),
    [actors]
  );

  const { nodes: graphNodes, edges } = React.useMemo(() => {
    if (actors.length === 0 && ports.length === 0) {
      return { nodes: [] as Node[], edges: [] as Edge[] };
    }

    const ACTOR_X_GAP = 300;
    const ACTOR_Y_GAP = 210;
    const TOOL_X_GAP = 220;
    const PORT_X_GAP = 240;
    const CLIENT_X_GAP = 260;
    const PORT_CLIENT_LAYER_Y = 0;
    const PORT_LAYER_Y = 170;
    const PORT_TO_ACTOR_GAP = 170;

    const actorIds = new Set(actors.map((actor) => actor.id));
    const actorById = new Map(actors.map((actor) => [actor.id, actor]));
    const portActors = new Set<string>();
    const actorPairs = new Map<string, [string, string]>();
    const actorOutgoing = new Map<string, Set<string>>();
    const actorIncoming = new Map<string, Set<string>>();
    const actorToolGroups = new Map<string, Set<string>>();
    const toolGroups = new Set<string>();

    for (const actor of actors) {
      actorOutgoing.set(actor.id, new Set<string>());
      actorIncoming.set(actor.id, new Set<string>());
    }

    for (const actor of actors) {
      const messages = actorGraphMessages[actor.id] ?? [];
      for (const message of messages) {
        if (isActorEventMessage(message)) {
          continue;
        }

        const toolName = extractToolName(message.payload);
        if (!toolName) {
          continue;
        }

        const toolGroup = normalizeToolGroup(toolName);
        toolGroups.add(toolGroup);
        const existingGroups =
          actorToolGroups.get(actor.id) ?? new Set<string>();
        existingGroups.add(toolGroup);
        actorToolGroups.set(actor.id, existingGroups);

        const normalizedToolName = toolName.toLowerCase();
        if (normalizedToolName.startsWith("actors-send")) {
          const targetActorId = extractTargetActorIdFromPayload(
            message.payload
          );
          if (
            targetActorId &&
            actorIds.has(targetActorId) &&
            targetActorId !== actor.id
          ) {
            const outgoing = actorOutgoing.get(actor.id);
            const incoming = actorIncoming.get(targetActorId);
            if (outgoing && incoming && !outgoing.has(targetActorId)) {
              outgoing.add(targetActorId);
              incoming.add(actor.id);
            }

            const [left, right] = [actor.id, targetActorId].sort();
            actorPairs.set(`${left}|${right}`, [left, right]);
          }
        }
      }
    }

    for (const port of ports) {
      for (const actorId of port.actorIds) {
        if (actorIds.has(actorId)) {
          portActors.add(actorId);
        }
      }
      if (port.assignedActorId && actorIds.has(port.assignedActorId)) {
        portActors.add(port.assignedActorId);
      }
    }

    const rootActorIds = actors
      .filter((actor) => {
        if (portActors.has(actor.id)) {
          return true;
        }
        return (actorIncoming.get(actor.id)?.size ?? 0) === 0;
      })
      .map((actor) => actor.id);
    if (rootActorIds.length === 0 && actors.length > 0) {
      rootActorIds.push(actors[0].id);
    }

    const actorDepth = new Map<string, number>();
    const queue: string[] = [];
    for (const rootId of rootActorIds) {
      if (!actorDepth.has(rootId)) {
        actorDepth.set(rootId, 0);
        queue.push(rootId);
      }
    }

    while (queue.length > 0) {
      const sourceId = queue.shift();
      if (!sourceId) {
        continue;
      }

      const sourceDepth = actorDepth.get(sourceId) ?? 0;
      const targets = actorOutgoing.get(sourceId) ?? new Set<string>();
      for (const targetId of targets) {
        const nextDepth = sourceDepth + 1;
        const existingDepth = actorDepth.get(targetId);
        if (existingDepth === undefined || nextDepth < existingDepth) {
          actorDepth.set(targetId, nextDepth);
          queue.push(targetId);
        }
      }
    }

    for (const actor of actors) {
      if (!actorDepth.has(actor.id)) {
        actorDepth.set(actor.id, 0);
      }
    }

    const actorsByDepth = new Map<number, ActorSummary[]>();
    for (const actor of actors) {
      const depth = actorDepth.get(actor.id) ?? 0;
      const existing = actorsByDepth.get(depth) ?? [];
      existing.push(actor);
      actorsByDepth.set(depth, existing);
    }

    const orderedDepths = [...actorsByDepth.keys()].sort(
      (left, right) => left - right
    );
    const orderedActorsByDepth = new Map<number, ActorSummary[]>();
    const rowIndexes = new Map<string, number>();

    for (const depth of orderedDepths) {
      const layerActors = [...(actorsByDepth.get(depth) ?? [])];
      if (depth > 0) {
        layerActors.sort((left, right) => {
          const leftParents = [
            ...(actorIncoming.get(left.id) ?? new Set<string>()),
          ];
          const rightParents = [
            ...(actorIncoming.get(right.id) ?? new Set<string>()),
          ];

          const score = (parents: string[]): number => {
            const indexes = parents
              .map((parentId) => rowIndexes.get(parentId))
              .filter((index): index is number => index !== undefined);
            if (indexes.length === 0) {
              return Number.MAX_SAFE_INTEGER;
            }
            return (
              indexes.reduce((sum, index) => sum + index, 0) / indexes.length
            );
          };

          const leftScore = score(leftParents);
          const rightScore = score(rightParents);
          if (leftScore !== rightScore) {
            return leftScore - rightScore;
          }
          return left.name.localeCompare(right.name);
        });
      }

      orderedActorsByDepth.set(depth, layerActors);
      layerActors.forEach((actor, index) => {
        rowIndexes.set(actor.id, index);
      });
    }

    const actorLayerMaxWidth = orderedDepths.reduce((maxWidth, depth) => {
      const rowSize = orderedActorsByDepth.get(depth)?.length ?? 0;
      const rowWidth = Math.max(0, (rowSize - 1) * ACTOR_X_GAP);
      return Math.max(maxWidth, rowWidth);
    }, 0);

    const sortedToolGroups = [...toolGroups].sort((left, right) =>
      left.localeCompare(right)
    );
    const sortedPorts = [...ports].sort((left, right) =>
      left.name.localeCompare(right.name)
    );
    const sortedPortClients: Array<{
      nodeId: string;
      portId: string;
      provider: string;
      conversationKey: string;
      label: string;
    }> = [];
    const seenClientNodeIds = new Set<string>();

    for (const port of sortedPorts) {
      const sortedBindings = [...port.bindings].sort((left, right) =>
        left.conversationKey.localeCompare(right.conversationKey)
      );
      for (const binding of sortedBindings) {
        const nodeId = `stage:client:${port.id}:${binding.id}`;
        if (seenClientNodeIds.has(nodeId)) {
          continue;
        }
        seenClientNodeIds.add(nodeId);
        sortedPortClients.push({
          nodeId,
          portId: port.id,
          provider: port.provider,
          conversationKey: binding.conversationKey,
          label: formatPortClientLabel(binding.conversationKey),
        });
      }
    }

    const toolRowWidth = Math.max(
      0,
      (sortedToolGroups.length - 1) * TOOL_X_GAP
    );
    const portRowWidth = Math.max(0, (sortedPorts.length - 1) * PORT_X_GAP);
    const clientRowWidth = Math.max(
      0,
      (sortedPortClients.length - 1) * CLIENT_X_GAP
    );
    const canvasWidth = Math.max(
      actorLayerMaxWidth,
      toolRowWidth,
      portRowWidth,
      clientRowWidth
    );
    const hasPortNodes = sortedPorts.length > 0;
    const actorOffsetY = hasPortNodes ? PORT_LAYER_Y + PORT_TO_ACTOR_GAP : 40;

    const actorNodes: Node[] = [];
    let maxActorDepth = 0;
    for (const depth of orderedDepths) {
      maxActorDepth = Math.max(maxActorDepth, depth);
      const layerActors = orderedActorsByDepth.get(depth) ?? [];
      const rowWidth = Math.max(0, (layerActors.length - 1) * ACTOR_X_GAP);
      const rowStartX = (canvasWidth - rowWidth) / 2;

      layerActors.forEach((actor, index) => {
        actorNodes.push({
          id: actor.id,
          type: "actor",
          position: {
            x: rowStartX + index * ACTOR_X_GAP,
            y: actorOffsetY + depth * ACTOR_Y_GAP,
          },
          data: { actor },
          selected: actor.id === selectedActorId,
        });
      });
    }

    const allNodes: Node[] = [];
    if (sortedPortClients.length > 0) {
      const startX = (canvasWidth - clientRowWidth) / 2;
      for (const [index, client] of sortedPortClients.entries()) {
        const isSelected = selectedPortId === client.portId;
        const stroke = colorForProvider(client.provider);
        allNodes.push({
          id: client.nodeId,
          position: {
            x: startX + index * CLIENT_X_GAP,
            y: PORT_CLIENT_LAYER_Y,
          },
          data: { label: client.label, portId: client.portId },
          selected: isSelected,
          draggable: false,
          style: {
            borderRadius: "999px",
            border: `1px solid ${isSelected ? stroke : "rgba(100, 116, 139, 0.45)"}`,
            background: "rgba(248, 250, 252, 0.96)",
            color: "rgb(30, 41, 59)",
            fontWeight: 600,
            fontSize: "11px",
            padding: "6px 10px",
          },
        });
      }
    }

    if (sortedPorts.length > 0) {
      const startX = (canvasWidth - portRowWidth) / 2;
      for (const [index, port] of sortedPorts.entries()) {
        const isSelected = selectedPortId === port.id;
        allNodes.push({
          id: `stage:port:${port.id}`,
          position: {
            x: startX + index * PORT_X_GAP,
            y: PORT_LAYER_Y,
          },
          data: { label: port.name },
          selected: isSelected,
          draggable: false,
          style: {
            borderRadius: "12px",
            border: `1px solid ${
              isSelected
                ? "rgba(14, 116, 214, 0.85)"
                : port.enabled
                  ? "rgba(14, 116, 214, 0.45)"
                  : "rgba(100, 116, 139, 0.35)"
            }`,
            background: port.enabled
              ? "rgba(239, 246, 255, 0.96)"
              : "rgba(248, 250, 252, 0.96)",
            color: port.enabled ? "rgb(30, 64, 175)" : "rgb(71, 85, 105)",
            fontWeight: 700,
            padding: "8px 12px",
          },
        });
      }
    }
    allNodes.push(...actorNodes);

    if (sortedToolGroups.length > 0) {
      const startX = (canvasWidth - toolRowWidth) / 2;
      const toolY = actorOffsetY + (maxActorDepth + 1) * ACTOR_Y_GAP + 40;
      for (const [index, toolGroup] of sortedToolGroups.entries()) {
        allNodes.push({
          id: `stage:tool:${toolGroup}`,
          position: {
            x: startX + index * TOOL_X_GAP,
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
    for (const client of sortedPortClients) {
      const port = sortedPorts.find((entry) => entry.id === client.portId);
      const stroke = colorForProvider(port?.provider ?? client.provider);
      allEdges.push({
        id: `stage:client-port:${client.nodeId}`,
        source: client.nodeId,
        target: `stage:port:${client.portId}`,
        style: { stroke, strokeWidth: 1.2 },
        animated: false,
      });
    }

    for (const port of ports) {
      const sourceId = `stage:port:${port.id}`;
      for (const actorId of port.actorIds) {
        if (!actorIds.has(actorId)) {
          continue;
        }

        const actor = actorById.get(actorId);
        const stroke = colorForProvider(actor?.provider ?? port.provider);
        allEdges.push({
          id: `stage:port:${port.id}:${actorId}`,
          source: sourceId,
          target: actorId,
          style: { stroke, strokeWidth: 1.5 },
          animated: port.enabled,
        });
      }
    }

    for (const [left, right] of actorPairs.values()) {
      const leftDepth = actorDepth.get(left) ?? 0;
      const rightDepth = actorDepth.get(right) ?? 0;
      const source = leftDepth <= rightDepth ? left : right;
      const target = source === left ? right : left;
      const sourceActor = actorById.get(source);

      allEdges.push({
        id: `stage:actor:${left}:${right}`,
        source,
        target,
        style: {
          stroke: colorForProvider(sourceActor?.provider ?? ""),
          strokeWidth: 1.6,
        },
      });
    }

    for (const [actorId, groups] of actorToolGroups.entries()) {
      const actor = actorById.get(actorId);
      const stroke = colorForProvider(actor?.provider ?? "");
      for (const group of groups) {
        allEdges.push({
          id: `stage:tool:${actorId}:${group}`,
          source: actorId,
          target: `stage:tool:${group}`,
          style: { stroke, strokeWidth: 1.3 },
          animated: true,
        });
      }
    }

    return { nodes: allNodes, edges: allEdges };
  }, [actorGraphMessages, actors, ports, selectedActorId, selectedPortId]);

  const [flowNodes, setFlowNodes] = React.useState<Node[]>([]);

  const toggleActorStatus = React.useCallback(
    async (actorId: string, newStatus: string) => {
      const actor = actors.find((entry) => entry.id === actorId);
      if (!actor) {
        return;
      }

      const nextStatus = parseActorStatusValue(newStatus);
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
              id: actor.id,
              name: actor.name,
              status: nextStatus,
              systemPrompt: actor.systemPrompt,
            },
          },
        });

        setActors((current) =>
          current.map((entry) =>
            entry.id === actor.id
              ? {
                  ...entry,
                  status: nextStatus,
                }
              : entry
          )
        );

        setActorDetailsDraft((current) =>
          current
            ? {
                ...current,
                status: nextStatus,
              }
            : current
        );
      } catch (err) {
        console.error("Failed to toggle actor status:", err);
      }
    },
    [actors]
  );

  React.useEffect(() => {
    setFlowNodes(
      graphNodes.map((node) => {
        if (node.type !== "actor") {
          return node;
        }

        const pinned = pinnedActorPositions[node.id];

        let updatedNode = node;
        if (pinned) {
          updatedNode = {
            ...node,
            position: pinned,
          };
        }

        return {
          ...updatedNode,
          data: {
            ...updatedNode.data,
            onToggleStatus: toggleActorStatus,
          },
        };
      })
    );
  }, [graphNodes, pinnedActorPositions, toggleActorStatus]);

  const onFlowNodesChange = React.useCallback(
    (changes: NodeChange[]) => {
      setFlowNodes((current) => applyNodeChanges(changes, current));
      setPinnedActorPositions((current) => {
        let changed = false;
        const next = { ...current };

        for (const change of changes) {
          if (change.type !== "position" || !change.position) {
            continue;
          }
          if (!actorIdSet.has(change.id)) {
            continue;
          }

          const previous = current[change.id];
          if (
            !previous ||
            previous.x !== change.position.x ||
            previous.y !== change.position.y
          ) {
            next[change.id] = change.position;
            changed = true;
          }
        }

        return changed ? next : current;
      });
    },
    [actorIdSet]
  );
  const selectedMessages = mailbox?.messages ?? [];
  const mergedMessages = React.useMemo(() => {
    const actorOptimistic = selectedActorId
      ? (optimisticMessages[selectedActorId] ?? [])
      : [];
    return [...selectedMessages, ...actorOptimistic].sort((left, right) =>
      left.createdAt.localeCompare(right.createdAt)
    );
  }, [optimisticMessages, selectedActorId, selectedMessages]);

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
        for (const field of toToolFields(
          extractToolArguments(message.payload)
        )) {
          fieldsByKey.set(field.key, field.value);
        }
        if (resultMessage) {
          for (const field of toToolFields(
            extractToolResult(resultMessage.payload)
          )) {
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
  const toolCallCount = mailboxEntries.filter(
    (entry) => entry.kind === "tool"
  ).length;
  const startedAgo = formatTimeSince(selectedActor?.createdAt ?? null);
  const newestMailboxEntryKey =
    mailboxEntries.length > 0
      ? (mailboxEntries[mailboxEntries.length - 1]?.key ?? "")
      : "";

  React.useEffect(() => {
    if (activeTab !== "mailbox" || !newestMailboxEntryKey) {
      return;
    }

    requestAnimationFrame(() => {
      scrollMailboxToBottom(false);
    });
  }, [activeTab, newestMailboxEntryKey, scrollMailboxToBottom]);

  const runtimeBadgeClass =
    runtimeStatus === "online"
      ? "bg-emerald-500/15 text-emerald-700"
      : runtimeStatus === "checking"
        ? "bg-amber-500/15 text-amber-700"
        : "bg-rose-500/15 text-rose-700";

  return (
    <div className="stage-shell h-screen w-screen overflow-hidden p-3 text-slate-900 md:p-4">
      <div className="stage-card relative flex h-full min-h-0 flex-col rounded-3xl border border-white/60 bg-white/70 shadow-xl">
        <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 p-3 lg:grid-cols-[1fr_32rem]">
          <section className="relative min-h-0 overflow-hidden rounded-2xl border border-slate-200 bg-white/85">
            <div className="pointer-events-none absolute left-3 top-3 z-20">
              <div className="pointer-events-auto inline-flex items-center gap-2 rounded-xl border border-slate-200 bg-white/95 px-3 py-1.5 shadow-sm backdrop-blur">
                <p className="text-[11px] uppercase tracking-[0.14em] text-slate-500">
                  Borg Actor Playground
                </p>
                <Badge className={runtimeBadgeClass}>{runtimeStatus}</Badge>
              </div>
            </div>

            <div className="pointer-events-none absolute right-3 top-3 z-20 flex items-center gap-2">
              {selectedActor ? (
                <Button
                  type="button"
                  variant="destructive"
                  className="pointer-events-auto"
                  onClick={() => void deleteActor()}
                  disabled={isDeletingActor}
                >
                  {isDeletingActor ? "Deleting..." : "Delete actor"}
                </Button>
              ) : null}
              <Button
                type="button"
                variant="outline"
                className="pointer-events-auto"
                onClick={() => setIsCreateActorOpen(true)}
              >
                + Actor
              </Button>
            </div>

            {isLoadingActors && flowNodes.length === 0 ? (
              <div className="flex h-full items-center justify-center text-sm text-slate-500">
                Loading actor graph...
              </div>
            ) : flowNodes.length === 0 ? (
              <div className="flex h-full items-center justify-center p-8 text-center text-sm text-slate-500">
                No graph nodes found. Create an actor or configure a port.
              </div>
            ) : (
              <ReactFlow
                className="stage-flow"
                nodes={flowNodes}
                edges={edges}
                nodeTypes={actorNodeTypes}
                onNodesChange={onFlowNodesChange}
                onNodeClick={(_event, node) => {
                  if (node.type === "actor") {
                    setSelectedActorId(node.id);
                    setSelectedPortId(null);
                    setActiveTab("mailbox");
                    return;
                  }

                  if (node.id.startsWith("stage:port:")) {
                    const portId = node.id.replace(/^stage:port:/, "");
                    setSelectedPortId(portId);
                    setSelectedActorId(null);
                    return;
                  }

                  if (node.id.startsWith("stage:client:")) {
                    const data = node.data as { portId?: string } | undefined;
                    if (data?.portId) {
                      setSelectedPortId(data.portId);
                      setSelectedActorId(null);
                    }
                  }
                }}
                fitView
                fitViewOptions={{ padding: 0.2 }}
                nodesDraggable
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
                  <div>
                    <p className="text-[11px] uppercase tracking-[0.14em] text-slate-500">
                      Actor
                    </p>
                    <h2 className="text-sm font-semibold">
                      {selectedActor.name}
                    </h2>
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

                        <article className="rounded-xl border border-slate-200 bg-white px-3 py-3">
                          <div className="flex items-center justify-between gap-2">
                            <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                              Actor Details
                            </p>
                            <span
                              className={`text-[10px] ${
                                actorDetailsDirty
                                  ? "text-amber-700"
                                  : "text-emerald-700"
                              }`}
                            >
                              {actorDetailsDirty ? "Unsaved changes" : "Saved"}
                            </span>
                          </div>

                          {actorDetailsDraft ? (
                            <div className="mt-2 grid gap-3">
                              <div className="space-y-1.5">
                                <Label htmlFor="stage-detail-name">Name</Label>
                                <Input
                                  id="stage-detail-name"
                                  value={actorDetailsDraft.name}
                                  onChange={(event) => {
                                    const nextValue = event.currentTarget.value;
                                    setActorDetailsDraft((current) =>
                                      current
                                        ? {
                                            ...current,
                                            name: nextValue,
                                          }
                                        : current
                                    );
                                  }}
                                />
                              </div>

                              <div className="space-y-1.5">
                                <Label htmlFor="stage-detail-status">
                                  Status
                                </Label>
                                <div
                                  id="stage-detail-status"
                                  className="flex items-center justify-between rounded-lg border border-slate-200 bg-slate-50 px-3 py-2"
                                >
                                  <p className="text-xs font-medium text-slate-700">
                                    {actorDetailsDraft.status ===
                                    ActorStatusValue.Running
                                      ? "RUNNING"
                                      : "PAUSED"}
                                  </p>
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={() =>
                                      void toggleActorStatus(
                                        selectedActor.id,
                                        actorDetailsDraft.status ===
                                          ActorStatusValue.Running
                                          ? "PAUSED"
                                          : "RUNNING"
                                      )
                                    }
                                  >
                                    {actorDetailsDraft.status ===
                                    ActorStatusValue.Running
                                      ? "Pause"
                                      : "Play"}
                                  </Button>
                                </div>
                              </div>

                              <div className="grid grid-cols-2 gap-3">
                                <div className="space-y-1.5">
                                  <Label htmlFor="stage-detail-provider">
                                    Provider
                                  </Label>
                                  <Combobox
                                    value={actorDetailsDraft.provider}
                                    onValueChange={(value) =>
                                      setActorDetailsDraft((current) =>
                                        current
                                          ? {
                                              ...current,
                                              provider:
                                                normalizeComboboxValue(value),
                                            }
                                          : current
                                      )
                                    }
                                  >
                                    <ComboboxInput
                                      id="stage-detail-provider"
                                      placeholder="Select provider"
                                    />
                                    <ComboboxContent>
                                      <ComboboxEmpty>
                                        No providers found
                                      </ComboboxEmpty>
                                      <ComboboxList>
                                        {providers.map((provider) => (
                                          <ComboboxItem
                                            key={provider.provider}
                                            value={provider.provider}
                                          >
                                            {provider.provider}
                                          </ComboboxItem>
                                        ))}
                                      </ComboboxList>
                                    </ComboboxContent>
                                  </Combobox>
                                </div>

                                <div className="space-y-1.5">
                                  <Label htmlFor="stage-detail-model">
                                    Model
                                  </Label>
                                  <Combobox
                                    value={actorDetailsDraft.model}
                                    onValueChange={(value) =>
                                      setActorDetailsDraft((current) =>
                                        current
                                          ? {
                                              ...current,
                                              model:
                                                normalizeComboboxValue(value),
                                            }
                                          : current
                                      )
                                    }
                                  >
                                    <ComboboxInput
                                      id="stage-detail-model"
                                      placeholder="Select model"
                                    />
                                    <ComboboxContent>
                                      <ComboboxEmpty>
                                        No models found
                                      </ComboboxEmpty>
                                      <ComboboxList>
                                        {detailsModelOptions.map((model) => (
                                          <ComboboxItem
                                            key={model}
                                            value={model}
                                          >
                                            {model}
                                          </ComboboxItem>
                                        ))}
                                      </ComboboxList>
                                    </ComboboxContent>
                                  </Combobox>
                                </div>
                              </div>

                              <div className="space-y-1.5">
                                <Label htmlFor="stage-detail-prompt">
                                  System Prompt
                                </Label>
                                <Textarea
                                  id="stage-detail-prompt"
                                  value={actorDetailsDraft.systemPrompt}
                                  onChange={(event) => {
                                    const nextValue = event.currentTarget.value;
                                    setActorDetailsDraft((current) =>
                                      current
                                        ? {
                                            ...current,
                                            systemPrompt: nextValue,
                                          }
                                        : current
                                    );
                                  }}
                                  className="min-h-28"
                                />
                              </div>

                              <div className="flex justify-end">
                                <Button
                                  type="button"
                                  onClick={() => void saveActorDetails()}
                                  disabled={
                                    isSavingActorDetails || !actorDetailsDirty
                                  }
                                >
                                  {isSavingActorDetails
                                    ? "Saving..."
                                    : "Save details"}
                                </Button>
                              </div>
                            </div>
                          ) : null}
                        </article>
                      </div>
                    </ScrollArea>
                  </TabsContent>

                  <TabsContent value="mailbox" className="mt-0 min-h-0 flex-1">
                    <div className="flex h-full min-h-0 flex-col">
                      <ScrollArea className="min-h-0 flex-1 px-3 py-3">
                        <div className="space-y-2">
                          {isLoadingMailbox && selectedMessages.length === 0 ? (
                            <p className="text-xs text-slate-500">
                              Loading mailbox...
                            </p>
                          ) : mailboxEntries.length ? (
                            mailboxEntries.map((entry) => {
                              if (entry.kind === "tool") {
                                const defaultExpanded =
                                  !shouldCollapseToolEntry(entry);
                                const isExpanded =
                                  expandedToolEntries[entry.key] ??
                                  defaultExpanded;
                                return (
                                  <article
                                    key={entry.key}
                                    className="mr-auto max-w-[94%] rounded-2xl border border-amber-300 bg-amber-50 px-3 py-2 text-xs shadow-sm"
                                  >
                                    <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-amber-800">
                                      <span>{entry.role ?? "tool"}</span>
                                      <span>{formatDate(entry.createdAt)}</span>
                                    </div>

                                    <div className="mb-1 flex items-center justify-between gap-2">
                                      <p className="text-[12px] font-semibold text-amber-900">
                                        {entry.toolName}
                                      </p>
                                      <Button
                                        type="button"
                                        size="icon-xs"
                                        variant="outline"
                                        onClick={() =>
                                          setExpandedToolEntries((current) => ({
                                            ...current,
                                            [entry.key]: !isExpanded,
                                          }))
                                        }
                                      >
                                        {isExpanded ? "-" : "+"}
                                      </Button>
                                    </div>

                                    {isExpanded && entry.fields.length > 0 ? (
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
                                    ) : isExpanded ? (
                                      <p className="text-[11px] text-amber-700">
                                        (no output)
                                      </p>
                                    ) : (
                                      <p className="whitespace-pre-wrap break-words text-[11px] text-amber-800">
                                        {summarizeToolEntry(entry)}
                                      </p>
                                    )}

                                    <p className="mt-1 text-[10px] text-amber-700">
                                      {entry.sourceType}
                                    </p>
                                  </article>
                                );
                              }

                              const message = entry.message;
                              const normalizedRole = (
                                message.role ?? ""
                              ).toLowerCase();
                              const isUser = normalizedRole === "user";
                              const isAssistant =
                                normalizedRole === "assistant";
                              const roleLabel = isAssistant
                                ? (selectedActor?.name ?? "assistant")
                                : (message.role ?? "system");
                              const parsedPayload = parseJsonPayload(
                                message.payload
                              );
                              const hasText =
                                (message.text ?? "").trim().length > 0;

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
                                    <span>{roleLabel}</span>
                                    <span>{formatDate(message.createdAt)}</span>
                                  </div>

                                  {hasText ? (
                                    <p className="whitespace-pre-wrap text-[12px] text-slate-700">
                                      {message.text}
                                    </p>
                                  ) : parsedPayload !== null &&
                                    parsedPayload !== undefined ? (
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
                        <Textarea
                          value={draft}
                          onChange={(event) =>
                            setDraft(event.currentTarget.value)
                          }
                          onKeyDown={(event) => {
                            if (
                              event.key === "Enter" &&
                              (event.metaKey || event.ctrlKey)
                            ) {
                              event.preventDefault();
                              void sendMessage();
                            }
                          }}
                          className="min-h-24"
                          placeholder="Type a message to this actor (Cmd/Ctrl + Enter to send)"
                        />
                        <div className="flex justify-end">
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
            ) : selectedPort ? (
              <div className="flex h-full min-h-0 flex-col">
                <header className="space-y-2 border-b border-slate-200 px-4 py-3">
                  <div>
                    <p className="text-[11px] uppercase tracking-[0.14em] text-slate-500">
                      Port
                    </p>
                    <h2 className="text-sm font-semibold">
                      {selectedPort.name}
                    </h2>
                  </div>
                  <p className="truncate text-[11px] text-slate-500">
                    {selectedPort.id}
                  </p>
                </header>

                <ScrollArea className="h-full px-3 py-3">
                  <div className="grid gap-3">
                    <article className="rounded-xl border border-slate-200 bg-white px-3 py-3">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                          Port Details
                        </p>
                        <span
                          className={`text-[10px] ${
                            portDetailsDirty
                              ? "text-amber-700"
                              : "text-emerald-700"
                          }`}
                        >
                          {portDetailsDirty ? "Unsaved changes" : "Saved"}
                        </span>
                      </div>

                      {portDetailsDraft ? (
                        <div className="mt-2 grid gap-3">
                          <div className="space-y-1.5">
                            <Label htmlFor="stage-port-name">Name</Label>
                            <input
                              id="stage-port-name"
                              type="text"
                              className="w-full rounded-lg border border-slate-200 px-3 py-2 text-xs text-slate-700 placeholder:text-slate-400"
                              placeholder="Enter port name"
                              value={portDetailsDraft.name}
                              onChange={(event) => {
                                setPortDetailsDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        name: event.currentTarget.value,
                                      }
                                    : current
                                );
                              }}
                            />
                          </div>

                          <div className="space-y-1.5">
                            <Label htmlFor="stage-port-provider">
                              Provider
                            </Label>
                            <Combobox
                              value={portDetailsDraft.provider}
                              onValueChange={(value) =>
                                setPortDetailsDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        provider: normalizeComboboxValue(value),
                                      }
                                    : current
                                )
                              }
                            >
                              <ComboboxInput
                                id="stage-port-provider"
                                placeholder="Select provider"
                              />
                              <ComboboxContent>
                                <ComboboxEmpty>
                                  No port providers found
                                </ComboboxEmpty>
                                <ComboboxList>
                                  {availablePortProviders.map((provider) => (
                                    <ComboboxItem
                                      key={provider}
                                      value={provider}
                                    >
                                      {provider}
                                    </ComboboxItem>
                                  ))}
                                </ComboboxList>
                              </ComboboxContent>
                            </Combobox>
                          </div>

                          <div className="space-y-1.5">
                            <Label htmlFor="stage-port-assigned-actor">
                              Default Actor Binding
                            </Label>
                            <Combobox
                              value={portDetailsDraft.assignedActorId}
                              onValueChange={(value) =>
                                setPortDetailsDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        assignedActorId:
                                          normalizeComboboxValue(value),
                                      }
                                    : current
                                )
                              }
                            >
                              <ComboboxInput
                                id="stage-port-assigned-actor"
                                placeholder="Select actor (optional)"
                              />
                              <ComboboxContent>
                                <ComboboxEmpty>No actors found</ComboboxEmpty>
                                <ComboboxList>
                                  <ComboboxItem value="">(none)</ComboboxItem>
                                  {actors.map((actor) => (
                                    <ComboboxItem
                                      key={actor.id}
                                      value={actor.id}
                                    >
                                      {actor.name} ({actor.id})
                                    </ComboboxItem>
                                  ))}
                                </ComboboxList>
                              </ComboboxContent>
                            </Combobox>
                          </div>

                          <div className="grid grid-cols-2 gap-3">
                            <label className="flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
                              <input
                                type="checkbox"
                                checked={portDetailsDraft.enabled}
                                onChange={(event) => {
                                  const isChecked = event.currentTarget.checked;
                                  setPortDetailsDraft((current) =>
                                    current
                                      ? {
                                          ...current,
                                          enabled: isChecked,
                                        }
                                      : current
                                  );
                                }}
                              />
                              <span className="text-xs font-medium text-slate-700">
                                Enabled
                              </span>
                            </label>
                            <label className="flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
                              <input
                                type="checkbox"
                                checked={portDetailsDraft.allowsGuests}
                                onChange={(event) => {
                                  const isChecked = event.currentTarget.checked;
                                  setPortDetailsDraft((current) =>
                                    current
                                      ? {
                                          ...current,
                                          allowsGuests: isChecked,
                                        }
                                      : current
                                  );
                                }}
                              />
                              <span className="text-xs font-medium text-slate-700">
                                Allows guests
                              </span>
                            </label>
                          </div>

                          <div className="flex justify-end">
                            <Button
                              type="button"
                              onClick={() => void savePortDetails()}
                              disabled={
                                isSavingPortDetails || !portDetailsDirty
                              }
                            >
                              {isSavingPortDetails ? "Saving..." : "Save port"}
                            </Button>
                          </div>
                        </div>
                      ) : null}
                    </article>

                    <article className="rounded-xl border border-slate-200 bg-white px-3 py-3">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-[11px] uppercase tracking-[0.08em] text-slate-500">
                          Conversation Actor Bindings
                        </p>
                        <span
                          className={`text-[10px] ${
                            dirtyPortBindingIds.size > 0
                              ? "text-amber-700"
                              : "text-emerald-700"
                          }`}
                        >
                          {dirtyPortBindingIds.size > 0
                            ? `${dirtyPortBindingIds.size} unsaved`
                            : "Saved"}
                        </span>
                      </div>
                      <div className="mt-2 space-y-2">
                        {selectedPort.actorBindings.length === 0 ? (
                          <p className="text-xs text-slate-500">
                            No explicit actor bindings for this port.
                          </p>
                        ) : (
                          selectedPort.actorBindings.map((binding) => (
                            <div
                              key={binding.id}
                              className="rounded-lg border border-slate-200 bg-slate-50 p-2"
                            >
                              <p className="truncate text-[11px] text-slate-500">
                                {binding.conversationKey}
                              </p>
                              <div className="mt-2 flex items-center gap-2">
                                <Combobox
                                  value={portBindingDrafts[binding.id] ?? ""}
                                  onValueChange={(value) =>
                                    setPortBindingDrafts((current) => ({
                                      ...current,
                                      [binding.id]:
                                        normalizeComboboxValue(value),
                                    }))
                                  }
                                >
                                  <ComboboxInput placeholder="Select actor (optional)" />
                                  <ComboboxContent>
                                    <ComboboxEmpty>
                                      No actors found
                                    </ComboboxEmpty>
                                    <ComboboxList>
                                      <ComboboxItem value="">
                                        (none)
                                      </ComboboxItem>
                                      {actors.map((actor) => (
                                        <ComboboxItem
                                          key={actor.id}
                                          value={actor.id}
                                        >
                                          {actor.name} ({actor.id})
                                        </ComboboxItem>
                                      ))}
                                    </ComboboxList>
                                  </ComboboxContent>
                                </Combobox>
                                <Button
                                  type="button"
                                  size="sm"
                                  onClick={() =>
                                    void savePortBinding(binding.id)
                                  }
                                  disabled={
                                    savingBindingKey === binding.id ||
                                    !dirtyPortBindingIds.has(binding.id)
                                  }
                                >
                                  {savingBindingKey === binding.id
                                    ? "Saving..."
                                    : "Save"}
                                </Button>
                              </div>
                            </div>
                          ))
                        )}
                      </div>
                    </article>
                  </div>
                </ScrollArea>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center p-6 text-center text-sm text-slate-500">
                Select an actor or port node to inspect details.
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
              Define the actor id, status, and system prompt used by the
              runtime.
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
                onChange={(event) => {
                  const nextValue = event.currentTarget.value;
                  setCreateActorDraft((current) => ({
                    ...current,
                    name: nextValue,
                  }));
                }}
                placeholder="Planner"
              />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-2">
                <Label htmlFor="stage-create-actor-provider">Provider</Label>
                <Combobox
                  value={createActorDraft.provider}
                  onValueChange={(value) =>
                    setCreateActorDraft((current) => ({
                      ...current,
                      provider: normalizeComboboxValue(value),
                    }))
                  }
                >
                  <ComboboxInput
                    id="stage-create-actor-provider"
                    placeholder="Select provider"
                  />
                  <ComboboxContent>
                    <ComboboxEmpty>No providers found</ComboboxEmpty>
                    <ComboboxList>
                      {providers.map((p) => (
                        <ComboboxItem key={p.provider} value={p.provider}>
                          {p.provider} ({p.providerKind})
                        </ComboboxItem>
                      ))}
                    </ComboboxList>
                  </ComboboxContent>
                </Combobox>
              </div>

              <div className="space-y-2">
                <Label htmlFor="stage-create-actor-model">Model</Label>
                <Combobox
                  value={createActorDraft.model}
                  onValueChange={(value) =>
                    setCreateActorDraft((current) => ({
                      ...current,
                      model: normalizeComboboxValue(value),
                    }))
                  }
                >
                  <ComboboxInput
                    id="stage-create-actor-model"
                    placeholder="Select model"
                  />
                  <ComboboxContent>
                    <ComboboxEmpty>No models found</ComboboxEmpty>
                    <ComboboxList>
                      {availableModels.map((m) => (
                        <ComboboxItem key={m} value={m}>
                          {m}
                        </ComboboxItem>
                      ))}
                    </ComboboxList>
                  </ComboboxContent>
                </Combobox>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="stage-create-actor-prompt">System Prompt</Label>
              <Textarea
                id="stage-create-actor-prompt"
                value={createActorDraft.systemPrompt}
                onChange={(event) => {
                  const nextValue = event.currentTarget.value;
                  setCreateActorDraft((current) => ({
                    ...current,
                    systemPrompt: nextValue,
                  }));
                }}
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
