import { Card, CardContent } from "@borg/ui";
import React from "react";

const recentTraces = [
  "trace_borg-session-telegram_2654566: ingress -> policy check -> tool chain -> response",
  "trace_borg-session-http_1f902: ingress -> queued task -> provider retry -> response",
  "trace_borg-session-telegram_998122: ingress -> memory lookup -> tool failure -> fallback response",
];

export function TracingSection() {
  return (
    <Card title="Tracing">
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          Trace execution across sessions, agents, tool calls, provider
          requests, and policy gates to debug behavior.
        </p>
        <div className="space-y-2">
          {recentTraces.map((trace) => (
            <p
              key={trace}
              className="rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground"
            >
              {trace}
            </p>
          ))}
        </div>
        <div className="rounded-md border px-3 py-2 text-xs text-muted-foreground">
          Planned: per-trace timeline view, span filtering by session/agent, and
          export for incident postmortems.
        </div>
      </CardContent>
    </Card>
  );
}
