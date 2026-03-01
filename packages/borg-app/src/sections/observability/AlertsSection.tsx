import { Badge, Card, CardContent } from "@borg/ui";
import React from "react";

const starterAlerts = [
  { metric: "P95 Latency", condition: "> 6s for 5m", status: "Enabled" },
  { metric: "Error Rate", condition: "> 3% for 10m", status: "Enabled" },
  {
    metric: "Queue Depth",
    condition: "> 75 messages for 10m",
    status: "Draft",
  },
];

export function AlertsSection() {
  return (
    <Card title="Alerts">
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          Configure thresholds on observability metrics so the team can react to
          regressions quickly.
        </p>
        <div className="space-y-2">
          {starterAlerts.map((alert) => (
            <div
              key={alert.metric}
              className="flex items-center justify-between rounded-md border px-3 py-2"
            >
              <div>
                <p className="text-sm font-medium">{alert.metric}</p>
                <p className="text-xs text-muted-foreground">
                  {alert.condition}
                </p>
              </div>
              <Badge
                variant={alert.status === "Enabled" ? "secondary" : "outline"}
              >
                {alert.status}
              </Badge>
            </div>
          ))}
        </div>
        <div className="rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground">
          Upcoming: destination routing (Slack/Telegram/webhook), silencing
          windows, and escalation policy chains.
        </div>
      </CardContent>
    </Card>
  );
}
