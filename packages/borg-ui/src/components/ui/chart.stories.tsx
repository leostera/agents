import type { Meta, StoryObj } from "@storybook/react-vite";
import { Bar, BarChart, CartesianGrid, Line, LineChart, XAxis } from "recharts";

import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "./chart";

const trafficData = [
  { month: "Jan", actors: 540, conversions: 142 },
  { month: "Feb", actors: 610, conversions: 175 },
  { month: "Mar", actors: 720, conversions: 198 },
  { month: "Apr", actors: 830, conversions: 236 },
  { month: "May", actors: 790, conversions: 220 },
  { month: "Jun", actors: 910, conversions: 274 },
];

const eventsData = [
  { day: "Mon", runs: 18 },
  { day: "Tue", runs: 24 },
  { day: "Wed", runs: 21 },
  { day: "Thu", runs: 29 },
  { day: "Fri", runs: 33 },
];

const chartConfig = {
  actors: {
    label: "Actors",
    color: "hsl(217 91% 60%)",
  },
  conversions: {
    label: "Conversions",
    color: "hsl(142 71% 45%)",
  },
  runs: {
    label: "Workflow Runs",
    color: "hsl(25 95% 53%)",
  },
} satisfies ChartConfig;

const meta: Meta<typeof ChartContainer> = {
  title: "UI/Chart",
  component: ChartContainer,
};

export default meta;
type Story = StoryObj<typeof ChartContainer>;

export const ConversionTrend: Story = {
  render: () => (
    <ChartContainer config={chartConfig} className="h-72 w-full">
      <LineChart data={trafficData} margin={{ left: 12, right: 12 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="month" tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent indicator="line" />} />
        <ChartLegend content={<ChartLegendContent />} />
        <Line
          dataKey="actors"
          type="monotone"
          stroke="var(--color-actors)"
          strokeWidth={2}
          dot={false}
        />
        <Line
          dataKey="conversions"
          type="monotone"
          stroke="var(--color-conversions)"
          strokeWidth={2}
          dot={false}
        />
      </LineChart>
    </ChartContainer>
  ),
};

export const WeeklyRuns: Story = {
  render: () => (
    <ChartContainer config={chartConfig} className="h-72 w-full">
      <BarChart data={eventsData} margin={{ left: 12, right: 12 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="day" tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent hideLabel />} />
        <Bar dataKey="runs" fill="var(--color-runs)" radius={8} />
      </BarChart>
    </ChartContainer>
  ),
};
