import {
  createBorgApiClient,
  type SessionRecord,
  type UserRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Label,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import { LoaderCircle, Save, Trash2 } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type UserDetailsPageProps = {
  userKey: string;
};

type SessionMessageRow = {
  sessionId: string;
  index: number;
  snippet: string;
};

function stringifyMessage(message: Record<string, unknown>): string {
  const content =
    typeof message.content === "string"
      ? message.content
      : typeof message.text === "string"
        ? message.text
        : JSON.stringify(message);
  return content.length > 180 ? `${content.slice(0, 180)}…` : content;
}

export function UserDetailsPage({ userKey }: UserDetailsPageProps) {
  const [user, setUser] = React.useState<UserRecord | null>(null);
  const [profileJson, setProfileJson] = React.useState("{}");
  const [sessions, setSessions] = React.useState<SessionRecord[]>([]);
  const [messages, setMessages] = React.useState<SessionMessageRow[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const loadUser = React.useCallback(async () => {
    if (!userKey.trim()) {
      setUser(null);
      setError("Missing user key");
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [foundUser, allSessions] = await Promise.all([
        borgApi.getUser(userKey),
        borgApi.listSessions(500),
      ]);

      if (!foundUser) {
        setUser(null);
        setError("User not found");
        return;
      }

      setUser(foundUser);
      setProfileJson(JSON.stringify(foundUser.profile ?? {}, null, 2));

      const matchingSessions = allSessions.filter((session) =>
        session.users.includes(userKey)
      );
      setSessions(matchingSessions.slice(0, 10));

      const messageResults = await Promise.all(
        matchingSessions.slice(0, 10).map(async (session) => {
          try {
            const rows = await borgApi.listSessionMessages(session.session_id, {
              from: 0,
              limit: 20,
            });
            return { sessionId: session.session_id, rows };
          } catch {
            return { sessionId: session.session_id, rows: [] };
          }
        })
      );

      const flattened: SessionMessageRow[] = [];
      for (const result of messageResults) {
        result.rows.forEach((row, index) => {
          if (row && typeof row === "object") {
            flattened.push({
              sessionId: result.sessionId,
              index,
              snippet: stringifyMessage(row as Record<string, unknown>),
            });
          }
        });
      }
      setMessages(flattened.slice(0, 20));
    } catch (loadError) {
      setUser(null);
      setSessions([]);
      setMessages([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load user"
      );
    } finally {
      setIsLoading(false);
    }
  }, [userKey]);

  React.useEffect(() => {
    void loadUser();
  }, [loadUser]);

  const handleSave = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    let profile: Record<string, unknown>;
    try {
      const parsed = JSON.parse(profileJson || "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Profile must be a JSON object");
      }
      profile = parsed as Record<string, unknown>;
    } catch (parseError) {
      setError(
        parseError instanceof Error ? parseError.message : "Invalid JSON"
      );
      return;
    }

    setIsSaving(true);
    try {
      await borgApi.patchUser(userKey, profile);
      await loadUser();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to save user"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDisable = async () => {
    setError(null);
    try {
      await borgApi.deleteUser(userKey, { ignoreNotFound: true });
      window.history.pushState(null, "", "/control/users");
      window.dispatchEvent(new PopStateEvent("popstate"));
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to disable user"
      );
    }
  };

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-sm">
        <LoaderCircle className="size-4 animate-spin" />
        Loading user...
      </p>
    );
  }

  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
      <section className="space-y-3">
        <section className="flex items-center justify-between gap-2">
          <Badge variant="outline" className="font-mono text-[11px]">
            {userKey}
          </Badge>
          <Button variant="outline" onClick={() => void handleDisable()}>
            <Trash2 className="size-4" />
            Disable
          </Button>
        </section>

        {error ? <p className="text-destructive text-xs">{error}</p> : null}

        {user ? (
          <form className="space-y-3" onSubmit={handleSave}>
            <div className="space-y-1">
              <Label htmlFor="user-key-readonly">User Key</Label>
              <Input id="user-key-readonly" value={user.user_key} readOnly />
            </div>
            <div className="space-y-1">
              <Label htmlFor="user-profile">Profile (JSON)</Label>
              <Textarea
                id="user-profile"
                value={profileJson}
                onChange={(event) => setProfileJson(event.currentTarget.value)}
                rows={12}
              />
            </div>
            <Button type="submit" disabled={isSaving}>
              {isSaving ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <Save className="size-4" />
              )}
              Save Changes
            </Button>
          </form>
        ) : (
          <p className="text-muted-foreground text-sm">User not found.</p>
        )}
      </section>

      <aside className="space-y-4 rounded-lg border p-3">
        <section className="space-y-2">
          <p className="text-sm font-semibold">Recent Sessions</p>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Session</TableHead>
                <TableHead>Updated</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sessions.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={2}
                    className="text-muted-foreground text-xs"
                  >
                    No recent sessions found.
                  </TableCell>
                </TableRow>
              ) : (
                sessions.map((session) => (
                  <TableRow key={session.session_id}>
                    <TableCell className="font-mono text-[11px]">
                      <Link href={`/control/sessions/${session.session_id}`}>
                        {session.session_id}
                      </Link>
                    </TableCell>
                    <TableCell className="text-xs">
                      {new Date(session.updated_at).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </section>

        <section className="space-y-2">
          <p className="text-sm font-semibold">Recent Messages</p>
          <div className="space-y-2">
            {messages.length === 0 ? (
              <p className="text-muted-foreground text-xs">
                No recent messages found.
              </p>
            ) : (
              messages.map((message) => (
                <div
                  key={`${message.sessionId}:${message.index}`}
                  className="rounded border p-2"
                >
                  <p className="font-mono text-[10px] text-muted-foreground">
                    {message.sessionId} #{message.index}
                  </p>
                  <p className="text-xs">{message.snippet}</p>
                </div>
              ))
            )}
          </div>
        </section>
      </aside>
    </section>
  );
}
