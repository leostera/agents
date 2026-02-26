import type { Meta, StoryObj } from '@storybook/react'

import { Kbd, KbdGroup } from './kbd'

const meta: Meta<typeof Kbd> = {
  title: 'UI/Kbd',
  component: Kbd,
  args: {
    children: 'K',
  },
}

export default meta
type Story = StoryObj<typeof Kbd>

export const Default: Story = {}

export const ShortcutGroup: Story = {
  render: () => (
    <div style={{ display: 'grid', gap: 8, fontSize: 12 }}>
      <span>Open command palette</span>
      <KbdGroup>
        <Kbd>Ctrl</Kbd>
        <Kbd>K</Kbd>
      </KbdGroup>
    </div>
  ),
}
