import { MailboxMessage } from "./types";

export function parseJsonPayload(payload: unknown): any {
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

export function asRecord(value: unknown): Record<string, any> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, any>;
}

export function pickString(
  object: Record<string, any> | null,
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

export function extractToolName(payload: unknown): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  return pickString(object, ["name", "tool_name", "toolName"]);
}

export function extractTargetActorIdFromPayload(
  payload: unknown
): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  if (!object) {
    return null;
  }

  const argsRaw =
    object.arguments ??
    object.arguments_json ??
    object.args ??
    object.input ??
    object.params;
  const args = asRecord(parseJsonPayload(argsRaw));

  if (args) {
    const fromArgs = pickString(args, [
      "targetActorId",
      "target_actor_id",
      "actorId",
      "actor_id",
      "receiverId",
      "receiver_id",
      "to",
    ]);
    if (fromArgs) return fromArgs;
  }

  return pickString(object, [
    "targetActorId",
    "target_actor_id",
    "actorId",
    "actor_id",
  ]);
}

export function extractSenderActorIdFromPayload(
  payload: unknown
): string | null {
  const parsed = parseJsonPayload(payload);
  const object = asRecord(parsed);
  if (!object) {
    return null;
  }

  // Handle structured actor_message
  if (object.type === "actor_message" || object.kind === "actor_message") {
    return pickString(object, ["sender_actor_id", "senderActorId", "from"]);
  }

  return null;
}

export function normalizeToolGroup(toolName: string): string {
  const trimmed = toolName.trim();
  if (!trimmed) return "Tool";
  const dash = trimmed.indexOf("-");
  if (dash > 0) return trimmed.slice(0, dash);
  const colon = trimmed.indexOf(":");
  if (colon > 0) return trimmed.slice(0, colon);
  return trimmed;
}

export function colorForProvider(provider: string): string {
  const normalized = provider.trim().toLowerCase();
  if (normalized === "openai") return "#0f766e";
  if (normalized === "openrouter") return "#1d4ed8";
  if (normalized.length === 0) return "#64748b";

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
  for (let i = 0; i < normalized.length; i++) {
    hash = (hash << 5) - hash + normalized.charCodeAt(i);
    hash |= 0;
  }
  return palette[Math.abs(hash) % palette.length] ?? "#64748b";
}

export function isActorEventMessage(message: MailboxMessage): boolean {
  return message.messageType.toLowerCase() === "actor_event";
}
