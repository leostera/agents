import { createBorgApiClient, type SessionRecord } from "@borg/api";
import { Badge, ChatThread, type ChatMessageItem } from "@borg/ui";
import React from "react";

const borgApi = createBorgApiClient();

type SessionDetailsPageProps = {
  sessionId: string;
};

export function SessionDetailsPage({ sessionId }: SessionDetailsPageProps) {
  const [session, setSession] = React.useState<SessionRecord | null>(null);
  const [messages, setMessages] = React.useState<Record<string, unknown>[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const chatMessages = React.useMemo<ChatMessageItem[]>(() => {
    return messages.map((message, index) =>
      mapSessionMessageToChatItem(message, index)
    );
  }, [messages]);

  React.useEffect(() => {
    if (!sessionId.trim()) {
      setError("Missing session id");
      setSession(null);
      setIsLoading(false);
      return;
    }

    let active = true;
    setIsLoading(true);
    setError(null);

    void (async () => {
      try {
        const row = await borgApi.getSession(sessionId);
        if (!active) return;
        setSession(row);
        if (!row) {
          setMessages([]);
          return;
        }

        const allMessages: Record<string, unknown>[] = [];
        const pageSize = 500;
        let from = 0;
        while (true) {
          const batch = await borgApi.listSessionMessages(sessionId, {
            from,
            limit: pageSize,
          });
          if (!active) return;
          if (batch.length === 0) break;
          allMessages.push(...batch);
          from += batch.length;
          if (batch.length < pageSize) break;
        }
        setMessages(allMessages);
      } catch (loadError) {
        if (!active) return;
        setSession(null);
        setMessages([]);
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load session"
        );
      } finally {
        if (!active) return;
        setIsLoading(false);
      }
    })();

    return () => {
      active = false;
    };
  }, [sessionId]);

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading session...</p>;
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  if (!session) {
    return <p className="text-muted-foreground text-sm">Session not found.</p>;
  }

  return (
    <section className="space-y-4">
      <section className="grid gap-3 md:grid-cols-2">
        <div>
          <p className="text-muted-foreground text-xs">Session ID</p>
          <p className="font-mono text-xs break-all">{session.session_id}</p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Users</p>
          <p className="font-mono text-xs break-all">
            {session.users.join(", ")}
          </p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Port</p>
          <Badge variant="outline">{session.port}</Badge>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Updated</p>
          <p>{new Date(session.updated_at).toLocaleString()}</p>
        </div>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Session Messages</p>
        <div className="h-[420px] rounded-xl border">
          <ChatThread
            messages={chatMessages}
            emptyTitle="No messages found"
            emptyDescription="This session has no chat history yet."
          />
        </div>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Raw Session JSON</p>
        <pre className="bg-muted/30 overflow-x-auto rounded-lg border p-3 text-xs leading-relaxed">
          {JSON.stringify(session, null, 2)}
        </pre>
      </section>
    </section>
  );
}

function mapSessionMessageToChatItem(
  payload: Record<string, unknown>,
  index: number
): ChatMessageItem {
  const role = detectMessageRole(payload);
  const text = extractMessageText(payload);
  const timestamp = formatMessageTimestamp(payload);
  const id = `session-message-${index}`;
  return { id, role, text: text.trim() || JSON.stringify(payload, null, 2), timestamp };
}

function detectMessageRole(
  payload: Record<string, unknown>
): ChatMessageItem["role"] {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const normalized = typeCandidate.trim().toLowerCase();
    if (normalized === "assistant") return "assistant";
    if (normalized === "user") return "user";
    if (normalized === "system") return "system";
    if (
      normalized === "tool_call" ||
      normalized === "tool_result" ||
      normalized === "session_event"
    ) {
      return "system";
    }
  }

  const candidates = [
    payload.role,
    payload.author,
    getNested(payload, ["message", "role"]),
    getNested(payload, ["payload", "role"]),
  ];
  for (const candidate of candidates) {
    if (typeof candidate !== "string") continue;
    const normalized = candidate.trim().toLowerCase();
    if (normalized === "assistant" || normalized === "agent")
      return "assistant";
    if (normalized === "user") return "user";
    if (normalized === "system") return "system";
  }

  const variantCandidates: Array<[string, ChatMessageItem["role"]]> = [
    ["assistant", "assistant"],
    ["user", "user"],
    ["system", "system"],
    ["toolcall", "system"],
    ["toolresult", "system"],
    ["sessionevent", "system"],
  ];
  for (const [variant, role] of variantCandidates) {
    if (hasTopLevelVariant(payload, variant)) return role;
  }

  return "system";
}

function extractMessageText(payload: Record<string, unknown>): string {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const normalized = typeCandidate.trim().toLowerCase();
    if (normalized === "tool_call") {
      const name = typeof payload.name === "string" ? payload.name : "tool";
      const argumentsValue = payload.arguments;
      const renderedArguments =
        argumentsValue === undefined ? "" : ` ${safeJson(argumentsValue)}`;
      return `Tool call: ${name}${renderedArguments}`;
    }
    if (normalized === "tool_result") {
      const name = typeof payload.name === "string" ? payload.name : "tool";
      const content = payload.content;
      if (typeof content === "string" && content.trim()) {
        return `Tool result: ${name}\n${content}`;
      }
      return `Tool result: ${name}\n${safeJson(content)}`;
    }
    if (normalized === "session_event") {
      const name = typeof payload.name === "string" ? payload.name : "event";
      return `Session event: ${name}\n${safeJson(payload.payload)}`;
    }
  }

  const candidates = [
    payload.content,
    payload.text,
    getNested(payload, ["message", "content"]),
    getNested(payload, ["payload", "text"]),
    getNested(payload, ["input", "text"]),
  ];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.trim()) {
      return candidate;
    }
  }

  const namedVariantText = extractFromVariantPayload(payload);
  if (namedVariantText) return namedVariantText;

  return JSON.stringify(payload, null, 2);
}

function formatMessageTimestamp(payload: Record<string, unknown>): string | null {
  const candidates = [
    payload.created_at,
    payload.timestamp,
    getNested(payload, ["message", "created_at"]),
  ];
  for (const candidate of candidates) {
    if (typeof candidate !== "string") continue;
    const date = new Date(candidate);
    if (!Number.isNaN(date.getTime())) {
      return date.toLocaleString();
    }
  }
  return null;
}

function getNested(
  payload: Record<string, unknown>,
  path: string[]
): unknown {
  let current: unknown = payload;
  for (const segment of path) {
    if (!current || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function hasTopLevelVariant(
  payload: Record<string, unknown>,
  variantName: string
): boolean {
  const normalizedVariant = variantName.trim().toLowerCase();
  return Object.keys(payload).some(
    (key) => key.trim().toLowerCase() === normalizedVariant
  );
}

function extractFromVariantPayload(payload: Record<string, unknown>): string | null {
  const variantMap: Array<[string, string]> = [
    ["user", "User"],
    ["assistant", "Assistant"],
    ["system", "System"],
    ["toolcall", "Tool call"],
    ["toolresult", "Tool result"],
    ["sessionevent", "Session event"],
  ];

  for (const [variant, label] of variantMap) {
    const key = Object.keys(payload).find(
      (candidate) => candidate.trim().toLowerCase() === variant
    );
    if (!key) continue;
    const value = payload[key];
    if (value === undefined) return label;
    if (typeof value === "string") return value;
    if (value && typeof value === "object") {
      const obj = value as Record<string, unknown>;
      if (typeof obj.content === "string" && obj.content.trim()) {
        return obj.content;
      }
      return `${label}\n${safeJson(obj)}`;
    }
    return `${label}\n${String(value)}`;
  }
  return null;
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
