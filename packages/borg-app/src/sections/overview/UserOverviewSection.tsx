import React from 'react'
import { Area, AreaChart, Bar, BarChart, CartesianGrid, XAxis } from 'recharts'
import {
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from '@borg/ui'
import { DollarSign, DoorOpen, Sparkles } from 'lucide-react'

const costBurnData = [
  { day: 'Feb 21', cost: 31.2 },
  { day: 'Feb 22', cost: 33.4 },
  { day: 'Feb 23', cost: 34.1 },
  { day: 'Feb 24', cost: 36.6 },
  { day: 'Feb 25', cost: 38.9 },
  { day: 'Feb 26', cost: 40.3 },
  { day: 'Feb 27', cost: 41.2 },
]

const providerSpendData = [
  { provider: 'OpenAI', spend: 489.2 },
  { provider: 'Anthropic', spend: 211.4 },
  { provider: 'OpenRouter', spend: 86.7 },
]

const recentlyLearnedFacts = [
  'Preferred language is TypeScript for tooling tasks.',
  'Primary deployment target is Fly.io for preview environments.',
  'User prefers terse status updates during long-running work.',
  'Most active conversation window is weekday mornings.',
]

const mostCommonMemories = [
  { topic: 'Provider setup', count: 38 },
  { topic: 'Task retries', count: 27 },
  { topic: 'Session routing', count: 22 },
  { topic: 'Policy tuning', count: 17 },
]

const costChartConfig = {
  cost: {
    label: 'Cost',
    color: 'hsl(25 95% 53%)',
  },
} satisfies ChartConfig

const providerChartConfig = {
  spend: {
    label: 'Spend',
    color: 'hsl(217 91% 60%)',
  },
} satisfies ChartConfig

export function UserOverviewSection() {
  return (
    <section className='space-y-4'>
      <div className='grid gap-4 md:grid-cols-2 xl:grid-cols-3'>
        <Card>
          <CardHeader>
            <CardDescription>Sessions</CardDescription>
            <CardTitle className='text-3xl font-semibold'>248</CardTitle>
          </CardHeader>
          <CardContent className='flex items-center gap-2 text-xs text-muted-foreground'>
            <DoorOpen className='h-3.5 w-3.5' />
            39 active now, 17 started today
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Provider Spend (30d)</CardDescription>
            <CardTitle className='text-3xl font-semibold'>$787.30</CardTitle>
          </CardHeader>
          <CardContent className='flex items-center gap-2 text-xs text-muted-foreground'>
            <DollarSign className='h-3.5 w-3.5' />
            OpenAI remains primary driver (62%)
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Cost Burn</CardDescription>
            <CardTitle className='text-3xl font-semibold'>$41.20/day</CardTitle>
          </CardHeader>
          <CardContent className='flex items-center gap-2 text-xs text-muted-foreground'>
            <Sparkles className='h-3.5 w-3.5' />
            +8.3% week over week
          </CardContent>
        </Card>
      </div>

      <div className='grid gap-4 xl:grid-cols-3'>
        <Card className='xl:col-span-2'>
          <CardHeader>
            <CardTitle>Cost Burn</CardTitle>
            <CardDescription>Daily spend trend across providers.</CardDescription>
          </CardHeader>
          <CardContent className='pt-2'>
            <ChartContainer config={costChartConfig} className='h-72 w-full'>
              <AreaChart data={costBurnData} margin={{ left: 8, right: 8 }}>
                <defs>
                  <linearGradient id='fillCostBurn' x1='0' y1='0' x2='0' y2='1'>
                    <stop offset='5%' stopColor='var(--color-cost)' stopOpacity={0.35} />
                    <stop offset='95%' stopColor='var(--color-cost)' stopOpacity={0.05} />
                  </linearGradient>
                </defs>
                <CartesianGrid vertical={false} />
                <XAxis dataKey='day' tickLine={false} axisLine={false} tickMargin={8} />
                <ChartTooltip
                  cursor={false}
                  content={
                    <ChartTooltipContent
                      formatter={(value) => <span className='font-medium tabular-nums'>${Number(value).toFixed(2)}</span>}
                      indicator='dot'
                    />
                  }
                />
                <Area dataKey='cost' type='monotone' fill='url(#fillCostBurn)' stroke='var(--color-cost)' strokeWidth={2.5} />
              </AreaChart>
            </ChartContainer>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Spend by Provider</CardTitle>
            <CardDescription>Current month distribution.</CardDescription>
          </CardHeader>
          <CardContent className='space-y-3'>
            <ChartContainer config={providerChartConfig} className='h-44 w-full'>
              <BarChart data={providerSpendData} margin={{ left: 8, right: 8 }}>
                <CartesianGrid vertical={false} />
                <XAxis dataKey='provider' tickLine={false} axisLine={false} tickMargin={8} />
                <ChartTooltip
                  cursor={false}
                  content={
                    <ChartTooltipContent
                      formatter={(value) => <span className='font-medium tabular-nums'>${Number(value).toFixed(2)}</span>}
                      indicator='dot'
                    />
                  }
                />
                <Bar dataKey='spend' fill='var(--color-spend)' radius={8} />
              </BarChart>
            </ChartContainer>
            <div className='space-y-2'>
              {providerSpendData.map((provider) => (
                <div key={provider.provider} className='flex items-center justify-between rounded-md border px-3 py-2 text-sm'>
                  <span>{provider.provider}</span>
                  <span className='font-medium'>${provider.spend.toFixed(2)}</span>
                </div>
              ))}
            </div>
            <Badge variant='outline'>User-facing usage and cost view</Badge>
          </CardContent>
        </Card>
      </div>

      <div className='grid gap-4 xl:grid-cols-2'>
        <Card>
          <CardHeader>
            <CardTitle>Recently Learned Facts</CardTitle>
            <CardDescription>Latest facts added to memory from your sessions.</CardDescription>
          </CardHeader>
          <CardContent className='space-y-2'>
            {recentlyLearnedFacts.map((fact) => (
              <p key={fact} className='rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground'>
                {fact}
              </p>
            ))}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Most Common Memories</CardTitle>
            <CardDescription>Topics showing up most often in retained memory.</CardDescription>
          </CardHeader>
          <CardContent className='space-y-2'>
            {mostCommonMemories.map((item) => (
              <div key={item.topic} className='flex items-center justify-between rounded-md border px-3 py-2 text-sm'>
                <span>{item.topic}</span>
                <Badge variant='secondary'>{item.count}</Badge>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
    </section>
  )
}
