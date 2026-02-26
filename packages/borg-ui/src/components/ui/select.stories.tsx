import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'

import { UiSelect } from './select'

const meta: Meta<typeof UiSelect> = {
  title: 'UI/Select',
  component: UiSelect,
  args: {
    value: 'telegram',
    options: [
      { label: 'telegram', value: 'telegram' },
      { label: 'http', value: 'http' },
    ],
  },
}

export default meta
type Story = StoryObj<typeof UiSelect>

export const Default: Story = {
  render: (args) => {
    const [value, setValue] = useState(args.value)
    return <UiSelect {...args} value={value} onValueChange={setValue} />
  },
}
