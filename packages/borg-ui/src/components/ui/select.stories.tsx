import type { Meta, StoryObj } from '@storybook/react-vite'
import { useState } from 'react'

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  UiSelect,
} from './select'

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

export const Primitive: Story = {
  render: () => {
    const [value, setValue] = useState('telegram')
    return (
      <Select value={value} onValueChange={setValue}>
        <SelectTrigger>
          <SelectValue placeholder='Choose a port' />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='telegram'>telegram</SelectItem>
          <SelectItem value='http'>http</SelectItem>
          <SelectItem value='cli'>cli</SelectItem>
        </SelectContent>
      </Select>
    )
  },
}
