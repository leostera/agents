import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'

import { Button } from './button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './collapsible'

const meta: Meta<typeof Collapsible> = {
  title: 'UI/Collapsible',
  component: Collapsible,
  args: {
    defaultOpen: false,
  },
}

export default meta
type Story = StoryObj<typeof Collapsible>

export const Default: Story = {
  render: (args) => {
    const [open, setOpen] = useState(Boolean(args.defaultOpen))

    return (
      <Collapsible open={open} onOpenChange={setOpen}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '12px' }}>
          <p style={{ margin: 0, fontSize: '12px' }}>Deployment summary</p>
          <CollapsibleTrigger asChild>
            <Button variant='outline' size='sm'>
              {open ? 'Hide details' : 'Show details'}
            </Button>
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent>
          <div style={{ marginTop: '8px', fontSize: '12px', lineHeight: 1.5 }}>
            12 checks passed. 1 warning remains for an outdated API key in staging.
          </div>
        </CollapsibleContent>
      </Collapsible>
    )
  },
}
