import { createBorgApiClient, type ToolCallRecord } from "@borg/api";
import {
  Button,
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
const TOOL_CALLS_PREFIX = "/observability/tracing/tool-calls/";

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function matchesTerm(call: ToolCallRecord, term: string): boolean {
  if (!term) return true;
  const haystack = [
    call.call_id,
    call.session_id,
    call.tool_name,
    call.error ?? "",
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(term);
}

function formatDate(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString();
}

function formatDuration(value?: number | null): string {
  if (value == null || Number.isNaN(value)) return "—";
  return `${value} ms`;
}

function formatJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function resolveCallIdFromPath(pathname: string): string | null {
  const normalized = pathname.replace(/\/+$/, "");
  if (
    normalized.startsWith(TOOL_CALLS_PREFIX) &&
    normalized.length > TOOL_CALLS_PREFIX.length
  ) {
    const encoded = normalized.slice(TOOL_CALLS_PREFIX.length);
    try {
      return decodeURIComponent(encoded);
    } catch {
      return encoded;
    }
  }
  return null;
}

function navigateTo(path: string) {
  window.history.pushState(null, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

function ToolCallDetailPage({ callId }: { callId: string }) {
  const [call, setCall] = React.useState<ToolCallRecord | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);

    void borgApi
      .getToolCall(callId)
      .then((row) => {
        if (!active) return;
        setCall(row);
      })
      .catch((loadError) => {
        if (!active) return;
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load tool call details"
        );
      })
      .finally(() => {
        if (!active) return;
        setIsLoading(false);
      });

    return () => {
      active = false;
    };
  }, [callId]);

  if (isLoading) {
    return (
      <p className="text-muted-foreground text-sm">Loading tool call...</p>
    );
  }
  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }
  if (!call) {
    return (
      <p className="text-muted-foreground text-sm">Tool call not found.</p>
    );
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigateTo("/observability/tracing/tool-calls")}
        >
          Back to Tool Calls
        </Button>
        <p className="font-mono text-[11px] text-muted-foreground">
          {call.call_id}
        </p>
      </div>

      <section className="rounded-md border bg-muted/20 p-3">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <p className="text-xs">
            <span className="text-muted-foreground">Call ID: </span>
            <span className="font-mono">{call.call_id}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Tool: </span>
            <span>{call.tool_name}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Session: </span>
            <span className="font-mono">{call.session_id}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Success: </span>
            <span>{call.success ? "true" : "false"}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Duration: </span>
            <span>{formatDuration(call.duration_ms)}</span>
          </p>
          <p className="text-xs sm:col-span-2 lg:col-span-3">
            <span className="text-muted-foreground">Error: </span>
            <span>{call.error ?? "—"}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Called At: </span>
            <span>{formatDate(call.called_at)}</span>
          </p>
        </div>
      </section>

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="min-h-[520px] rounded-md border bg-muted/20">
          <div className="border-b px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Arguments JSON
          </div>
          <pre className="h-[480px] overflow-auto p-3 font-mono text-[11px] leading-5">
            {formatJson(call.arguments_json)}
          </pre>
        </section>
        <section className="min-h-[520px] rounded-md border bg-muted/20">
          <div className="border-b px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Output JSON
          </div>
          <pre className="h-[480px] overflow-auto p-3 font-mono text-[11px] leading-5">
            {formatJson(call.output_json)}
          </pre>
        </section>
      </div>
    </section>
  );
}

function ToolCallsListPage() {
  const [calls, setCalls] = React.useState<ToolCallRecord[]>([]);
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
      .listToolCalls(2000)
      .then((rows) => {
        if (!active) return;
        setCalls(rows);
      })
      .catch((loadError) => {
        if (!active) return;
        setCalls([]);
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load tool calls"
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
      ? `/observability/tracing/tool-calls?${paramsString}`
      : "/observability/tracing/tool-calls";
    window.history.replaceState(null, "", nextUrl);
  }, [query]);

  const filteredCalls = React.useMemo(() => {
    const term = normalize(query);
    return calls.filter((call) => matchesTerm(call, term));
  }, [calls, query]);

  return (
    <section className="space-y-4">
      <Input
        value={query}
        onChange={(event) => setQuery(event.currentTarget.value)}
        placeholder="Search by call id, session, tool name, or error"
        aria-label="Search tool calls"
      />
      {error ? <p className="text-destructive text-xs">{error}</p> : null}
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Call ID</TableHead>
            <TableHead>Called</TableHead>
            <TableHead>Tool</TableHead>
            <TableHead>Session</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Duration</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {isLoading ? (
            <TableRow>
              <TableCell
                colSpan={6}
                className="text-muted-foreground text-center"
              >
                Loading tool calls...
              </TableCell>
            </TableRow>
          ) : filteredCalls.length === 0 ? (
            <TableRow>
              <TableCell
                colSpan={6}
                className="text-muted-foreground text-center"
              >
                No tool calls found.
              </TableCell>
            </TableRow>
          ) : (
            filteredCalls.map((call) => (
              <TableRow
                key={call.call_id}
                className="cursor-pointer"
                onClick={() =>
                  navigateTo(
                    `/observability/tracing/tool-calls/${encodeURIComponent(call.call_id)}`
                  )
                }
              >
                <TableCell className="font-mono text-[11px]">
                  {call.call_id}
                </TableCell>
                <TableCell className="text-xs">
                  {formatDate(call.called_at)}
                </TableCell>
                <TableCell>{call.tool_name}</TableCell>
                <TableCell className="font-mono text-[11px]">
                  <Link
                    href={`/control/sessions/${encodeURIComponent(call.session_id)}`}
                    onClick={(event) => event.stopPropagation()}
                  >
                    {call.session_id}
                  </Link>
                </TableCell>
                <TableCell>{call.success ? "Success" : "Failed"}</TableCell>
                <TableCell>{formatDuration(call.duration_ms)}</TableCell>
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </section>
  );
}

export function ObservabilityToolCallsPage() {
  const callId = resolveCallIdFromPath(window.location.pathname);
  if (callId) return <ToolCallDetailPage callId={callId} />;
  return <ToolCallsListPage />;
}
