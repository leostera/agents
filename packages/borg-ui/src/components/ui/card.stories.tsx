import type { Meta, StoryObj } from '@storybook/react'

import { Card } from './card'

const meta: Meta<typeof Card> = {
  title: 'UI/Card',
  component: Card,
  args: {
    title: 'Users',
  },
}

export default meta
type Story = StoryObj<typeof Card>

export const Default: Story = {
  args: {
    children: (
      <p style={{ margin: 0, color: '#94a3b8' }}>
        This is a story for a dashboard card body.
      </p>
    ),
  },
}
