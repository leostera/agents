import type { Meta, StoryObj } from '@storybook/react'

import { Button } from './button'

const meta: Meta<typeof Button> = {
  title: 'UI/Button',
  component: Button,
  args: {
    children: 'Save',
    tone: 'default',
  },
}

export default meta
type Story = StoryObj<typeof Button>

export const Default: Story = {}

export const Subtle: Story = {
  args: {
    tone: 'subtle',
    children: 'Cancel',
  },
}
