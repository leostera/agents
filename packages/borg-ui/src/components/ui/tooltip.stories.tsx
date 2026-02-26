import type { Meta, StoryObj } from '@storybook/react'

import { Button } from './button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './tooltip'

const meta: Meta<typeof Tooltip> = {
  title: 'UI/Tooltip',
  component: Tooltip,
}

export default meta
type Story = StoryObj<typeof Tooltip>

export const Default: Story = {
  render: () => (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button size='sm' variant='outline'>
            Hover for status
          </Button>
        </TooltipTrigger>
        <TooltipContent sideOffset={6}>All systems operational</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  ),
}
