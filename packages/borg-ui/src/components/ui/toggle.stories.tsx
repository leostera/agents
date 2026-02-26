import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { TextBIcon } from '@phosphor-icons/react'

import { Toggle } from './toggle'

const meta: Meta<typeof Toggle> = {
  title: 'UI/Toggle',
  component: Toggle,
  args: {
    variant: 'default',
    size: 'default',
  },
}

export default meta
type Story = StoryObj<typeof Toggle>

export const Default: Story = {
  render: (args) => {
    const [pressed, setPressed] = useState(false)
    return (
      <Toggle {...args} pressed={pressed} onPressedChange={setPressed} aria-label='Bold'>
        <TextBIcon />
        Bold
      </Toggle>
    )
  },
}

export const Outline: Story = {
  args: {
    variant: 'outline',
    defaultPressed: true,
    children: 'Enabled filter',
    'aria-label': 'Enabled filter',
  },
}
