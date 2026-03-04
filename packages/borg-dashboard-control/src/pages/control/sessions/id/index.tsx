import {
  createBorgApiClient,
  type SessionContextRecord,
  type SessionRecord,
} from "@borg/api";
import { ChatThread, EntityLink, JsonTreeViewer } from "@borg/ui";
import React from "react";

const borgApi = createBorgApiClient();

type SessionDetailsPageProps = {
  sessionId: string;
};

export function SessionDetailsPage({ sessionId }: SessionDetailsPageProps) {
  const [session, setSession] = React.useState<SessionRecord | null>(null);
  const [sessionContext, setSessionContext] =
    React.useState<SessionContextRecord | null>(null);
  const [messages, setMessages] = React.useState<Record<string, unknown>[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const chatMessages = React.useMemo(() => {
    return toChatMessages(messages);
  }, [messages]);

  React.useEffect(() => {
    if (!sessionId.trim()) {
      setError("Missing session id");
      setSession(null);
      setSessionContext(null);
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
          setSessionContext(null);
          setMessages([]);
          return;
        }

        const context = await borgApi.getSessionContext(sessionId);
        if (!active) return;
        setSessionContext(context);

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
        setSessionContext(null);
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
    <section className="flex h-screen max-h-screen min-h-0 flex-col gap-4 overflow-hidden">
      <section className="rounded-lg border bg-card p-3">
        <div className="grid gap-3 md:grid-cols-4">
          <div>
            <p className="text-muted-foreground text-xs">Session ID</p>
            <p className="font-mono text-xs break-all">{session.session_id}</p>
          </div>
          <div>
            <p className="text-muted-foreground text-xs">Port</p>
            <EntityLink
              uri={session.port}
              name={portNameFromUri(session.port)}
            />
          </div>
          <div>
            <p className="text-muted-foreground text-xs">Users</p>
            <p className="font-mono text-xs break-all">
              {session.users.join(", ")}
            </p>
          </div>
          <div>
            <p className="text-muted-foreground text-xs">Updated</p>
            <p className="font-mono text-xs break-all">
              {new Date(session.updated_at).toLocaleString()}
            </p>
          </div>
        </div>
      </section>

      <section className="grid min-h-0 flex-1 gap-4 lg:grid-cols-2">
        <section className="flex min-h-0 flex-col gap-2">
          <p className="text-sm font-semibold">Messages</p>
          <div className="min-h-0 flex-1 overflow-hidden rounded-xl border bg-card">
            <ChatThread
              messages={chatMessages}
              emptyTitle="No messages found"
              emptyDescription="This session has no chat history yet."
            />
          </div>
        </section>

        <section className="flex min-h-0 flex-col gap-2">
          <p className="text-sm font-semibold">Context</p>
          <div className="bg-muted/30 min-h-0 flex-1 overflow-auto rounded-lg border p-2">
            <JsonTreeViewer
              value={sessionContext ?? { message: "No session context found." }}
              defaultExpandedDepth={2}
            />
          </div>
        </section>
      </section>
    </section>
  );
}

type SessionChatMessage = {
  id: string;
  role: "assistant" | "user" | "system";
  text: string;
  timestamp: string;
};

function detectMessageRole(
  payload: Record<string, unknown>
): SessionChatMessage["role"] {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const type = typeCandidate.trim().toLowerCase();
    if (type === "assistant") return "assistant";
    if (type === "user") return "user";
    if (type === "system") return "system";
    if (
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    )
      return "system";
  }

  const roleCandidate =
    typeof payload.role === "string"
      ? payload.role.trim().toLowerCase()
      : typeof payload.author === "string"
        ? payload.author.trim().toLowerCase()
        : null;
  if (roleCandidate) {
    if (roleCandidate === "assistant") return "assistant";
    if (roleCandidate === "user") return "user";
  }
  return "system";
}

function extractMessageText(payload: Record<string, unknown>): string {
  if (typeof payload.content === "string" && payload.content.trim()) {
    return payload.content;
  }
  if (typeof payload.text === "string" && payload.text.trim()) {
    return payload.text;
  }
  if (payload.type === "tool_call") {
    const name = typeof payload.name === "string" ? payload.name : "tool";
    return `Tool call: ${name}`;
  }
  if (payload.type === "tool_result") {
    const name = typeof payload.name === "string" ? payload.name : "tool";
    return `Tool result: ${name}`;
  }
  return safeJson(payload);
}

function isChatPayload(payload: Record<string, unknown>): boolean {
  const typeCandidate = payload.type;
  if (typeof typeCandidate === "string") {
    const type = typeCandidate.trim().toLowerCase();
    if (type === "user" || type === "assistant") return true;
    if (type === "system") return false;
    if (
      type === "tool_call" ||
      type === "tool_result" ||
      type === "session_event"
    )
      return false;
  }

  const roleCandidate =
    typeof payload.role === "string"
      ? payload.role.trim().toLowerCase()
      : typeof payload.author === "string"
        ? payload.author.trim().toLowerCase()
        : null;
  if (roleCandidate) {
    if (roleCandidate === "assistant" || roleCandidate === "user") {
      return true;
    }
    return false;
  }

  return (
    typeof payload.content === "string" || typeof payload.text === "string"
  );
}

function toChatMessages(
  rawMessages: Record<string, unknown>[]
): SessionChatMessage[] {
  const seen = new Set<string>();
  const mapped = rawMessages
    .filter((raw) => isChatPayload(raw as Record<string, unknown>))
    .map((raw, index) => {
      const payload = raw as Record<string, unknown>;
      const rawTimestamp =
        typeof payload.created_at === "string"
          ? payload.created_at
          : typeof payload.timestamp === "string"
            ? payload.timestamp
            : typeof payload.updated_at === "string"
              ? payload.updated_at
              : null;
      const role = detectMessageRole(payload);
      const text = extractMessageText(payload);
      const timestamp = rawTimestamp
        ? formatDate(rawTimestamp)
        : formatDate(new Date().toISOString());
      const messageIdentity =
        (typeof payload.message_id === "string" && payload.message_id.trim()) ||
        `${role}|${text}|${timestamp}`;
      return {
        id: `session-message-${index}`,
        role,
        text,
        timestamp,
        messageIdentity,
      };
    });

  return mapped
    .filter((message) => {
      if (seen.has(message.messageIdentity)) {
        return false;
      }
      seen.add(message.messageIdentity);
      return true;
    })
    .map(({ messageIdentity: _messageIdentity, ...message }) => message);
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function portNameFromUri(portUri: string): string {
  const trimmed = portUri.trim();
  if (!trimmed) return "unknown";
  const parts = trimmed.split(":");
  const name = parts.at(-1)?.trim();
  return name && name.length > 0 ? name : trimmed;
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
