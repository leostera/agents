import {
  Badge,
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  ToggleGroup,
  ToggleGroupItem,
} from "@borg/ui";
import {
  AlertTriangle,
  Bot,
  Clock3,
  DollarSign,
  ShieldCheck,
  User,
} from "lucide-react";
import React from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Line,
  LineChart,
  XAxis,
} from "recharts";

type RangeKey = "7d" | "30d" | "90d";

type ObservabilityPoint = {
  date: string;
  inbound: number;
  completed: number;
  p50: number;
  p95: number;
  errorRate: number;
  cost: number;
};

const RANGE_DAYS: Record<RangeKey, number> = {
  "7d": 7,
  "30d": 30,
  "90d": 90,
};

const REFERENCE_DATE = new Date("2026-02-27T00:00:00Z");

const observabilityData: ObservabilityPoint[] = Array.from(
  { length: 90 },
  (_, index) => {
    const date = new Date(REFERENCE_DATE);
    date.setDate(date.getDate() - (89 - index));

    const inbound =
      170 + ((index * 19) % 130) + Math.round(Math.sin(index / 4) * 22);
    const completed = inbound - (8 + (index % 14));
    const p50 =
      1.4 + ((index % 10) * 0.05 + Math.abs(Math.sin(index / 7)) * 0.2);
    const p95 = 3.8 + ((index % 9) * 0.1 + Math.abs(Math.cos(index / 8)) * 0.6);
    const errorRate =
      1.1 + ((index % 7) * 0.12 + Math.abs(Math.sin(index / 5)) * 0.5);
    const cost = 26 + ((index * 7) % 17) + Math.abs(Math.sin(index / 6)) * 11;

    return {
      date: date.toISOString().slice(0, 10),
      inbound,
      completed,
      p50: Number(p50.toFixed(2)),
      p95: Number(p95.toFixed(2)),
      errorRate: Number(errorRate.toFixed(2)),
      cost: Number(cost.toFixed(2)),
    };
  }
);

const throughputChartConfig = {
  inbound: {
    label: "Inbound",
    color: "hsl(217 91% 60%)",
  },
  completed: {
    label: "Completed",
    color: "hsl(168 76% 42%)",
  },
} satisfies ChartConfig;

const latencyChartConfig = {
  p50: {
    label: "P50 Latency (s)",
    color: "hsl(217 91% 60%)",
  },
  p95: {
    label: "P95 Latency (s)",
    color: "hsl(344 82% 58%)",
  },
} satisfies ChartConfig;

const costChartConfig = {
  cost: {
    label: "Cost",
    color: "hsl(25 95% 53%)",
  },
} satisfies ChartConfig;

const providerTokenLoad = [
  { provider: "OpenAI", tokens: "2.8M", share: "62%" },
  { provider: "Anthropic", tokens: "1.2M", share: "27%" },
  { provider: "OpenRouter", tokens: "0.5M", share: "11%" },
];

const runtimeChecks = [
  { label: "API", status: "Healthy" },
  { label: "Session Router", status: "Healthy" },
  { label: "LLM Providers", status: "Degraded" },
  { label: "Memory Index", status: "Healthy" },
];

const incidents = [
  "3 provider timeouts in the last 30 minutes.",
  "One session exceeded policy budget threshold.",
  "Ingress spikes detected on Telegram messages.",
];

export function OverviewSection() {
  const [timeRange, setTimeRange] = React.useState<RangeKey>("30d");

  const filteredData = React.useMemo(() => {
    const days = RANGE_DAYS[timeRange];
    return observabilityData.slice(-days);
  }, [timeRange]);

  const latest = filteredData[filteredData.length - 1];
  const prev = filteredData[Math.max(filteredData.length - 2, 0)];

  const throughputNow = latest?.completed ?? 0;
  const throughputDelta =
    latest && prev ? latest.completed - prev.completed : 0;
  const p95Now = latest?.p95 ?? 0;
  const errorNow = latest?.errorRate ?? 0;
  const costNow = latest?.cost ?? 0;

  return (
    <section className="space-y-4">
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <Card>
          <CardHeader>
            <CardDescription>Busiest Actor</CardDescription>
            <CardTitle className="text-xl font-semibold">
              `borg:actor:default`
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Runs (24h)</span>
              <span className="font-medium">184</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Avg duration</span>
              <span className="font-medium">42s</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Bot className="h-3.5 w-3.5" />
              13% more load than next actor
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Last Session Snapshot</CardDescription>
            <CardTitle className="text-xl font-semibold">
              `borg:session:telegram_2654566`
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Port</span>
              <span className="font-medium">telegram</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Last activity</span>
              <span className="font-medium">1m ago</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <User className="h-3.5 w-3.5" />6 user messages, 4 tool calls, 1
              follow-up
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Busiest Providers by Tokens</CardDescription>
            <CardTitle className="text-xl font-semibold">
              4.5M tokens (24h)
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {providerTokenLoad.map((item) => (
              <div
                key={item.provider}
                className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
              >
                <span>{item.provider}</span>
                <span className="font-medium">
                  {item.tokens}{" "}
                  <span className="text-xs text-muted-foreground">
                    ({item.share})
                  </span>
                </span>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-5">
        <Card size="sm">
          <CardHeader>
            <CardDescription>Turn Throughput</CardDescription>
            <CardTitle className="text-2xl font-semibold">
              {throughputNow}/h
            </CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            {throughputDelta >= 0 ? "+" : ""}
            {throughputDelta} from previous day
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>P95 Latency</CardDescription>
            <CardTitle className="text-2xl font-semibold">
              {p95Now.toFixed(1)}s
            </CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            end-to-end
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Error Rate</CardDescription>
            <CardTitle className="text-2xl font-semibold">
              {errorNow.toFixed(2)}%
            </CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            provider + runtime
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Queue Depth</CardDescription>
            <CardTitle className="text-2xl font-semibold">37</CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            median wait 3m 42s
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Cost Burn</CardDescription>
            <CardTitle className="text-2xl font-semibold">
              ${costNow.toFixed(2)}
            </CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <DollarSign className="h-3.5 w-3.5" />
            daily runtime spend
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader>
            <CardTitle>Ingress vs Completed Turns</CardTitle>
            <CardDescription>
              Volume trend by day across platform traffic.
            </CardDescription>
            <CardAction>
              <div className="flex items-center gap-2">
                <ToggleGroup
                  type="single"
                  value={timeRange}
                  onValueChange={(value) => {
                    if (value === "7d" || value === "30d" || value === "90d")
                      setTimeRange(value);
                  }}
                  variant="outline"
                  className="hidden md:flex"
                >
                  <ToggleGroupItem value="90d">90d</ToggleGroupItem>
                  <ToggleGroupItem value="30d">30d</ToggleGroupItem>
                  <ToggleGroupItem value="7d">7d</ToggleGroupItem>
                </ToggleGroup>
                <Select
                  value={timeRange}
                  onValueChange={(value: RangeKey) => setTimeRange(value)}
                >
                  <SelectTrigger
                    className="w-24 md:hidden"
                    aria-label="Select chart range"
                  >
                    <SelectValue placeholder="30d" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="90d">90 days</SelectItem>
                    <SelectItem value="30d">30 days</SelectItem>
                    <SelectItem value="7d">7 days</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </CardAction>
          </CardHeader>
          <CardContent className="pt-2">
            <ChartContainer
              config={throughputChartConfig}
              className="h-72 w-full"
            >
              <AreaChart data={filteredData} margin={{ left: 8, right: 8 }}>
                <defs>
                  <linearGradient id="fillInbound" x1="0" y1="0" x2="0" y2="1">
                    <stop
                      offset="5%"
                      stopColor="var(--color-inbound)"
                      stopOpacity={0.35}
                    />
                    <stop
                      offset="95%"
                      stopColor="var(--color-inbound)"
                      stopOpacity={0.06}
                    />
                  </linearGradient>
                  <linearGradient
                    id="fillCompleted"
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="1"
                  >
                    <stop
                      offset="5%"
                      stopColor="var(--color-completed)"
                      stopOpacity={0.3}
                    />
                    <stop
                      offset="95%"
                      stopColor="var(--color-completed)"
                      stopOpacity={0.05}
                    />
                  </linearGradient>
                </defs>
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  minTickGap={30}
                  tickFormatter={(value: string) =>
                    new Date(value).toLocaleDateString("en-US", {
                      month: "short",
                      day: "numeric",
                    })
                  }
                />
                <ChartTooltip
                  cursor={false}
                  content={
                    <ChartTooltipContent
                      labelFormatter={(value) =>
                        new Date(String(value)).toLocaleDateString("en-US", {
                          month: "short",
                          day: "numeric",
                        })
                      }
                      indicator="dot"
                    />
                  }
                />
                <Area
                  dataKey="inbound"
                  type="natural"
                  fill="url(#fillInbound)"
                  stroke="var(--color-inbound)"
                  strokeWidth={2}
                />
                <Area
                  dataKey="completed"
                  type="natural"
                  fill="url(#fillCompleted)"
                  stroke="var(--color-completed)"
                  strokeWidth={2}
                />
              </AreaChart>
            </ChartContainer>
          </CardContent>
        </Card>

        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardDescription>Runtime Status</CardDescription>
              <CardTitle>Health Checks</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {runtimeChecks.map((item) => (
                <div
                  key={item.label}
                  className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
                >
                  <span>{item.label}</span>
                  <Badge
                    variant={
                      item.status === "Healthy" ? "secondary" : "outline"
                    }
                  >
                    {item.status}
                  </Badge>
                </div>
              ))}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardDescription>Open Incidents</CardDescription>
              <CardTitle>Action Needed</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {incidents.map((incident) => (
                <div
                  key={incident}
                  className="flex items-start gap-2 rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground"
                >
                  <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span>{incident}</span>
                </div>
              ))}
            </CardContent>
          </Card>
        </div>
      </div>

      <div className="grid gap-4 xl:grid-cols-3">
        <Card className="xl:col-span-2">
          <CardHeader>
            <CardTitle>Latency Trend</CardTitle>
            <CardDescription>
              P50 and P95 response latency for completed turns.
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-2">
            <ChartContainer config={latencyChartConfig} className="h-64 w-full">
              <LineChart data={filteredData} margin={{ left: 8, right: 8 }}>
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  minTickGap={30}
                  tickFormatter={(value: string) =>
                    new Date(value).toLocaleDateString("en-US", {
                      month: "short",
                      day: "numeric",
                    })
                  }
                />
                <ChartTooltip
                  cursor={false}
                  content={
                    <ChartTooltipContent
                      formatter={(value, name) => (
                        <span className="font-medium tabular-nums">
                          {name}: {Number(value).toFixed(2)}s
                        </span>
                      )}
                      indicator="line"
                    />
                  }
                />
                <Line
                  dataKey="p50"
                  type="monotone"
                  stroke="var(--color-p50)"
                  strokeWidth={2.2}
                  dot={false}
                />
                <Line
                  dataKey="p95"
                  type="monotone"
                  stroke="var(--color-p95)"
                  strokeWidth={2.2}
                  dot={false}
                />
              </LineChart>
            </ChartContainer>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Cost Burn</CardDescription>
            <CardTitle className="text-3xl font-semibold">
              ${costNow.toFixed(2)}
            </CardTitle>
            <CardAction>
              <Badge variant="outline">last 24h</Badge>
            </CardAction>
          </CardHeader>
          <CardContent className="pt-1">
            <ChartContainer config={costChartConfig} className="h-44 w-full">
              <LineChart
                data={filteredData}
                margin={{ left: 4, right: 8, top: 6, bottom: 0 }}
              >
                <CartesianGrid vertical={false} />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  minTickGap={32}
                  tickFormatter={(value: string) =>
                    new Date(value).toLocaleDateString("en-US", {
                      month: "short",
                      day: "numeric",
                    })
                  }
                />
                <ChartTooltip
                  cursor={false}
                  content={
                    <ChartTooltipContent
                      formatter={(value) => (
                        <span className="font-medium tabular-nums">
                          ${Number(value).toFixed(2)}
                        </span>
                      )}
                      indicator="line"
                    />
                  }
                />
                <Line
                  dataKey="cost"
                  type="monotone"
                  stroke="var(--color-cost)"
                  strokeWidth={2.5}
                  dot={false}
                />
              </LineChart>
            </ChartContainer>
            <div className="mt-3 flex items-center justify-between rounded-md border border-dashed px-3 py-2 text-sm">
              <span className="text-muted-foreground">Projected 24h total</span>
              <span className="font-medium">$48.90</span>
            </div>
            <div className="mt-2 flex items-center justify-between rounded-md border px-3 py-2 text-sm">
              <span className="text-muted-foreground">
                Model retry overhead
              </span>
              <span className="font-medium">6.2%</span>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <Card size="sm">
          <CardHeader>
            <CardDescription>Tool Success Rate</CardDescription>
            <CardTitle className="text-2xl font-semibold">96.4%</CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            Top failures: CodeMode.executeCode, CodeMode.searchApis
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Policy Coverage</CardDescription>
            <CardTitle className="text-2xl font-semibold">92%</CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <ShieldCheck className="h-3.5 w-3.5" />8 entities missing explicit
            policy
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Port Lag</CardDescription>
            <CardTitle className="text-2xl font-semibold">1.3s</CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            telegram ingress to enqueue
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardDescription>Queue Age (P95)</CardDescription>
            <CardTitle className="text-2xl font-semibold">6m 12s</CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Clock3 className="h-3.5 w-3.5" />
            retry pressure increasing
          </CardContent>
        </Card>
      </div>
    </section>
  );
}
