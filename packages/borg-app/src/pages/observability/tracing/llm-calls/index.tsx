import { createBorgApiClient, type LlmCallRecord } from "@borg/api";
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
const LLM_CALLS_PREFIX = "/observability/tracing/llm-calls/";

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function matchesTerm(call: LlmCallRecord, term: string): boolean {
  if (!term) return true;
  const statusCode = call.status_code == null ? "" : String(call.status_code);
  const haystack = [
    call.call_id,
    call.provider,
    call.capability,
    call.model,
    call.status_reason ?? "",
    call.http_reason ?? "",
    call.error ?? "",
    statusCode,
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

function formatLatency(value?: number | null): string {
  if (value == null || Number.isNaN(value)) return "—";
  return `${value} ms`;
}

function formatStatus(call: LlmCallRecord): string {
  if (call.success) return "Success";
  if (call.status_code == null) return "Failed";
  return `HTTP ${call.status_code}`;
}

function formatValue(
  value: string | number | boolean | null | undefined
): string {
  if (value == null) return "—";
  if (typeof value === "string" && value.trim().length === 0) return "—";
  return String(value);
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
    normalized.startsWith(LLM_CALLS_PREFIX) &&
    normalized.length > LLM_CALLS_PREFIX.length
  ) {
    const encoded = normalized.slice(LLM_CALLS_PREFIX.length);
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

function LlmCallDetailPage({ callId }: { callId: string }) {
  const [call, setCall] = React.useState<LlmCallRecord | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);

    void borgApi
      .getLlmCall(callId)
      .then((row) => {
        if (!active) return;
        setCall(row);
      })
      .catch((loadError) => {
        if (!active) return;
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load LLM call details"
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
    return <p className="text-muted-foreground text-sm">Loading LLM call…</p>;
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  if (!call) {
    return <p className="text-muted-foreground text-sm">LLM call not found.</p>;
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigateTo("/observability/tracing/llm-calls")}
        >
          Back to LLM Calls
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
            <span className="text-muted-foreground">Provider: </span>
            <span>{call.provider}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Capability: </span>
            <span>{call.capability}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Model: </span>
            <span>{call.model}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Success: </span>
            <span>{call.success ? "true" : "false"}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Status: </span>
            <span>{formatStatus(call)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Status Code: </span>
            <span>{formatValue(call.status_code)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Status Reason: </span>
            <span>{formatValue(call.status_reason)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">HTTP Reason: </span>
            <span>{formatValue(call.http_reason)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Error: </span>
            <span>{formatValue(call.error)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Latency: </span>
            <span>{formatLatency(call.latency_ms)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Sent At: </span>
            <span>{formatDate(call.sent_at)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">Received At: </span>
            <span>{formatDate(call.received_at)}</span>
          </p>
          <p className="text-xs">
            <span className="text-muted-foreground">
              Response Body Length:{" "}
            </span>
            <span>{call.response_body.length}</span>
          </p>
        </div>
      </section>

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="min-h-[520px] rounded-md border bg-muted/20">
          <div className="border-b px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Request JSON
          </div>
          <pre className="h-[480px] overflow-auto p-3 font-mono text-[11px] leading-5">
            {formatJson(call.request_json)}
          </pre>
        </section>

        <section className="min-h-[520px] rounded-md border bg-muted/20">
          <div className="border-b px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Response JSON
          </div>
          <pre className="h-[480px] overflow-auto p-3 font-mono text-[11px] leading-5">
            {formatJson(call.response_json)}
          </pre>
          {!call.success && call.response_body ? (
            <>
              <div className="border-y px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Response Body
              </div>
              <pre className="max-h-64 overflow-auto p-3 font-mono text-[11px] leading-5">
                {call.response_body}
              </pre>
            </>
          ) : null}
        </section>
      </div>
    </section>
  );
}

function LlmCallsListPage() {
  const [calls, setCalls] = React.useState<LlmCallRecord[]>([]);
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
      .listLlmCalls(2000)
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
            : "Unable to load LLM calls"
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
      ? `/observability/tracing/llm-calls?${paramsString}`
      : "/observability/tracing/llm-calls";
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
        placeholder="Search by provider, model, call id, status, or reason"
        aria-label="Search llm calls"
      />

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Call ID</TableHead>
            <TableHead>Sent</TableHead>
            <TableHead>Provider</TableHead>
            <TableHead>Capability</TableHead>
            <TableHead>Model</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Reason</TableHead>
            <TableHead>Latency</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {isLoading ? (
            <TableRow>
              <TableCell
                colSpan={8}
                className="text-muted-foreground text-center"
              >
                Loading LLM calls...
              </TableCell>
            </TableRow>
          ) : filteredCalls.length === 0 ? (
            <TableRow>
              <TableCell
                colSpan={8}
                className="text-muted-foreground text-center"
              >
                No LLM calls found.
              </TableCell>
            </TableRow>
          ) : (
            filteredCalls.map((call) => (
              <TableRow
                key={call.call_id}
                className="cursor-pointer"
                onClick={() =>
                  navigateTo(
                    `/observability/tracing/llm-calls/${encodeURIComponent(call.call_id)}`
                  )
                }
              >
                <TableCell className="font-mono text-[11px]">
                  {call.call_id}
                </TableCell>
                <TableCell className="text-xs">
                  {formatDate(call.sent_at)}
                </TableCell>
                <TableCell>
                  <Link
                    href={`/settings/provider/${encodeURIComponent(call.provider)}`}
                    onClick={(event) => event.stopPropagation()}
                  >
                    {call.provider}
                  </Link>
                </TableCell>
                <TableCell>{call.capability}</TableCell>
                <TableCell>{call.model}</TableCell>
                <TableCell>{formatStatus(call)}</TableCell>
                <TableCell className="max-w-[420px] truncate">
                  {call.http_reason ?? call.error ?? call.status_reason ?? "—"}
                </TableCell>
                <TableCell>{formatLatency(call.latency_ms)}</TableCell>
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </section>
  );
}

export function ObservabilityLlmCallsPage() {
  const callId = resolveCallIdFromPath(window.location.pathname);
  if (callId) {
    return <LlmCallDetailPage callId={callId} />;
  }
  return <LlmCallsListPage />;
}
