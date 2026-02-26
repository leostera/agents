import type { Meta, StoryObj } from '@storybook/react'

import { Badge } from './badge'
import { ScrollArea } from './scroll-area'

const meta: Meta<typeof ScrollArea> = {
  title: 'UI/Scroll Area',
  component: ScrollArea,
}

export default meta
type Story = StoryObj<typeof ScrollArea>

const logs = [
  'Connected to provider: OpenAI',
  'Created session: session_4db2',
  'Queued tool call: list_ports',
  'Received tool output: 3 ports',
  'Dispatched follow-up prompt',
  'Agent selected model: gpt-4.1-mini',
  'Generated response: 281 tokens',
  'Persisted message to sqlite',
  'Task queue depth: 2',
  'Heartbeat healthy',
]

export const ActivityFeed: Story = {
  render: () => (
    <ScrollArea className='h-56 w-full max-w-lg border rounded-lg'>
      <div className='p-3 space-y-2'>
        {logs.map((log, index) => (
          <div key={log} className='border rounded-md p-2 text-xs/relaxed'>
            <div className='flex items-center justify-between mb-1'>
              <Badge variant='outline'>#{index + 1}</Badge>
              <span className='text-muted-foreground text-xs'>just now</span>
            </div>
            {log}
          </div>
        ))}
      </div>
    </ScrollArea>
  ),
}

export const HorizontalArtifacts: Story = {
  render: () => (
    <ScrollArea className='w-[420px] border rounded-lg'>
      <div className='flex gap-3 p-3 w-[820px]'>
        {['Prompt', 'Context', 'Tools', 'Output'].map((title) => (
          <div key={title} className='w-48 border rounded-md p-2 text-xs/relaxed'>
            <p className='font-medium mb-1'>{title}</p>
            <p className='text-muted-foreground'>
              Snapshot payload for the latest generation cycle.
            </p>
          </div>
        ))}
      </div>
    </ScrollArea>
  ),
}
