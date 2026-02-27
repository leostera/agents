import { createBorgApiClient, type SessionRecord } from "@borg/api";
import {
  Input,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import React from "react";

const borgApi = createBorgApiClient();

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function matchesTerm(session: SessionRecord, term: string): boolean {
  if (!term) return true;
  const haystack = [session.session_id, session.port, ...session.users]
    .join(" ")
    .toLowerCase();
  return haystack.includes(term);
}

export function SessionPage() {
  const [sessions, setSessions] = React.useState<SessionRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );

  React.useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);

    void borgApi
      .listSessions(10000)
      .then((rows) => {
        if (!active) return;
        setSessions(rows);
      })
      .catch((loadError) => {
        if (!active) return;
        setSessions([]);
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load sessions"
        );
      })
      .finally(() => {
        if (!active) return;
        setIsLoading(false);
      });

    return () => {
      active = false;
    };
  }, []);

  React.useEffect(() => {
    const params = new URLSearchParams();
    if (query.trim()) params.set("q", query.trim());
    const paramsString = params.toString();
    const nextUrl = paramsString
      ? `/control/sessions?${paramsString}`
      : "/control/sessions";
    window.history.replaceState(null, "", nextUrl);
  }, [query]);

  const filteredSessions = React.useMemo(() => {
    const term = normalize(query);
    return sessions.filter((session) => matchesTerm(session, term));
  }, [query, sessions]);

  return (
    <section className="space-y-4">
      <section>
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search sessions by id, users, or port"
          aria-label="Search sessions"
        />
      </section>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <section>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Session ID</TableHead>
              <TableHead>Users</TableHead>
              <TableHead>Providers</TableHead>
              <TableHead>Port</TableHead>
              <TableHead>Updated</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  Loading sessions...
                </TableCell>
              </TableRow>
            ) : filteredSessions.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  No sessions found.
                </TableCell>
              </TableRow>
            ) : (
              filteredSessions.map((session) => (
                <TableRow
                  key={session.session_id}
                  className="cursor-pointer"
                  onClick={() => {
                    window.history.pushState(
                      null,
                      "",
                      `/control/sessions/${session.session_id}`
                    );
                    window.dispatchEvent(new PopStateEvent("popstate"));
                  }}
                >
                  <TableCell className="font-mono text-[11px]">
                    <Link
                      href={`/control/sessions/${session.session_id}`}
                      onClick={(event) => event.stopPropagation()}
                    >
                      {session.session_id}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {session.users.join(", ")}
                  </TableCell>
                  <TableCell>—</TableCell>
                  <TableCell>{session.port}</TableCell>
                  <TableCell>
                    {new Date(session.updated_at).toLocaleString()}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>
    </section>
  );
}
