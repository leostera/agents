import { createBorgApiClient, type SessionRecord } from "@borg/api";
import { Badge } from "@borg/ui";
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
        <p className="text-sm font-semibold">
          Session Messages (Chronological)
        </p>
        {messages.length === 0 ? (
          <p className="text-muted-foreground text-sm">No messages found.</p>
        ) : (
          <div className="space-y-2">
            {messages.map((message, index) => (
              <article
                key={`${session.session_id}-${index}`}
                className="rounded-lg border p-3"
              >
                <p className="text-muted-foreground mb-2 text-[11px]">
                  #{index + 1}
                </p>
                <pre className="bg-muted/30 overflow-x-auto rounded-lg border p-3 text-xs leading-relaxed">
                  {JSON.stringify(message, null, 2)}
                </pre>
              </article>
            ))}
          </div>
        )}
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
