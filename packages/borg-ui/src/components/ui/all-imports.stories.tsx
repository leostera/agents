import type { Meta, StoryObj } from '@storybook/react'

const modules = import.meta.glob(['./*.tsx', '!./*.stories.tsx'], { eager: true })

const meta: Meta = {
  title: 'UI/All Imports Smoke',
}

export default meta
type Story = StoryObj<typeof meta>

export const ImportsEveryUiModule: Story = {
  render: () => <div>Imported {Object.keys(modules).length} UI modules</div>,
}
