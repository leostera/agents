import type { Meta, StoryObj } from '@storybook/react'
import { Bar, BarChart, CartesianGrid, Line, LineChart, XAxis } from 'recharts'

import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from './chart'

const trafficData = [
  { month: 'Jan', sessions: 540, conversions: 142 },
  { month: 'Feb', sessions: 610, conversions: 175 },
  { month: 'Mar', sessions: 720, conversions: 198 },
  { month: 'Apr', sessions: 830, conversions: 236 },
  { month: 'May', sessions: 790, conversions: 220 },
  { month: 'Jun', sessions: 910, conversions: 274 },
]

const eventsData = [
  { day: 'Mon', runs: 18 },
  { day: 'Tue', runs: 24 },
  { day: 'Wed', runs: 21 },
  { day: 'Thu', runs: 29 },
  { day: 'Fri', runs: 33 },
]

const chartConfig = {
  sessions: {
    label: 'Sessions',
    color: 'hsl(217 91% 60%)',
  },
  conversions: {
    label: 'Conversions',
    color: 'hsl(142 71% 45%)',
  },
  runs: {
    label: 'Workflow Runs',
    color: 'hsl(25 95% 53%)',
  },
} satisfies ChartConfig

const meta: Meta<typeof ChartContainer> = {
  title: 'UI/Chart',
  component: ChartContainer,
}

export default meta
type Story = StoryObj<typeof ChartContainer>

export const ConversionTrend: Story = {
  render: () => (
    <ChartContainer config={chartConfig} className='h-72 w-full'>
      <LineChart data={trafficData} margin={{ left: 12, right: 12 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey='month' tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent indicator='line' />} />
        <ChartLegend content={<ChartLegendContent />} />
        <Line
          dataKey='sessions'
          type='monotone'
          stroke='var(--color-sessions)'
          strokeWidth={2}
          dot={false}
        />
        <Line
          dataKey='conversions'
          type='monotone'
          stroke='var(--color-conversions)'
          strokeWidth={2}
          dot={false}
        />
      </LineChart>
    </ChartContainer>
  ),
}

export const WeeklyRuns: Story = {
  render: () => (
    <ChartContainer config={chartConfig} className='h-72 w-full'>
      <BarChart data={eventsData} margin={{ left: 12, right: 12 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey='day' tickLine={false} axisLine={false} />
        <ChartTooltip content={<ChartTooltipContent hideLabel />} />
        <Bar dataKey='runs' fill='var(--color-runs)' radius={8} />
      </BarChart>
    </ChartContainer>
  ),
}
